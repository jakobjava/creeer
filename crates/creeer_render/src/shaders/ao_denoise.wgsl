// GTAO, pass 3: the spatial filter of Jimenez et al. 2016 (§5), a 4×4 bilateral mean. The
// main pass turns its slices by a 4×4 interleaved pattern (Keller and Heidrich 2001), so any
// 4×4 window holds every turn once and the mean over it cancels the pattern exactly where the
// surface is flat. XeGTAO's 3×3 denoise instead leans on temporal anti-aliasing, which the game
// does not have. Pixels off this pixel's surface (by depth, against its plane) are left out.

@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var depths: texture_2d<f32>;
@group(0) @binding(3) var out_final: texture_storage_2d<rgba8unorm, write>;

fn depth_at(p: vec2<i32>) -> f32 {
    let level = i32(ao.base_mip);
    let last = vec2<i32>(textureDimensions(depths, level)) - 1;
    return textureLoad(depths, clamp(p, vec2<i32>(0), last), level).r;
}

fn ao_at(p: vec2<i32>) -> f32 {
    let last = vec2<i32>(ao.size) - 1;
    return textureLoad(source, clamp(p, vec2<i32>(0), last), 0).r;
}

/// The slope across one pixel: of the two one-sided differences, the smaller, so an edge on
/// one side does not tilt the plane.
fn slope(before: f32, centre: f32, after: f32) -> f32 {
    let back = centre - before;
    let ahead = after - centre;
    return select(ahead, back, abs(back) < abs(ahead));
}

// Contact shadows (G2.11, ADR-0040; after Bend Studio's screen-space shadows, Days Gone 2019):
// from each pixel, a short ray towards the sun through the screen's depth. What the shadow map
// is too coarse or too biased to catch (the line where a thing meets the ground, the shadow of
// a stick or a stone) is found here, sharp to the pixel; the scene takes the darker of the two.
const CONTACT_STEPS: i32 = 16;
// The longest the ray may be on the screen (pixels).
const CONTACT_MAX_PIXELS: f32 = 40.0;
// How far the ray goes (m): the shadows of things up to about a hand high.
const CONTACT_LENGTH: f32 = 0.3;
// How thick a surface seen on the screen is taken to be (m), and how much more per metre away.
const CONTACT_THICKNESS: f32 = 0.06;
const CONTACT_THICKNESS_PER_M: f32 = 0.004;

/// The full-resolution view depth at pixel `q` (MIP 0 of the chain).
fn full_depth(q: vec2<i32>) -> f32 {
    let last = vec2<i32>(textureDimensions(depths, 0)) - 1;
    return textureLoad(depths, clamp(q, vec2<i32>(0), last), 0).r;
}

/// 1 where the sun reaches the surface seen at grid pixel `p` as far as the screen can tell,
/// down to 0 where something within `CONTACT_LENGTH` of it stands in the way.
fn contact_light(p: vec2<i32>) -> f32 {
    if ao.sun.w <= 0.0 {
        return 1.0;
    }
    let full = ao.size * exp2(ao.base_mip);
    let screen = (vec2<f32>(p) + 0.5) * ao.pixel;
    let z = full_depth(vec2<i32>(screen * full));
    if z >= FAR_DEPTH {
        return 1.0;
    }
    let origin = view_position(screen, z);
    var towards = ao.sun.xyz * CONTACT_LENGTH;
    // How many pixels the ray crosses: too few, and it only finds its own surface; near the
    // camera it is shortened to `CONTACT_MAX_PIXELS`, so its steps stay a few pixels apart
    // (longer steps drew a near stick's shadow as stairs).
    let reach = (origin.xy + towards.xy) / max(origin.z + towards.z, ao.near) - origin.xy / z;
    let pixels = length(reach / ao.ndc_to_view_mul * full);
    if pixels < 2.0 {
        return 1.0;
    }
    towards *= min(1.0, CONTACT_MAX_PIXELS / pixels);
    var shadow = 0.0;
    for (var i = 1; i <= CONTACT_STEPS; i++) {
        let t = f32(i) / f32(CONTACT_STEPS);
        let q = origin + towards * t;
        if q.z <= ao.near {
            break;
        }
        let s = (q.xy / q.z - ao.ndc_to_view_add) / ao.ndc_to_view_mul;
        if any(s < vec2<f32>(0.0)) || any(s >= vec2<f32>(1.0)) {
            break;
        }
        let behind = q.z - full_depth(vec2<i32>(s * full));
        // In front of the ray point, but not so far in front that it is a thin thing well
        // before it (what lies behind a thin thing is not in its shadow); the start leaves
        // a margin for the depth's own rounding.
        let margin = 0.002 + 0.0005 * q.z;
        if behind > margin && behind < CONTACT_THICKNESS + CONTACT_THICKNESS_PER_M * q.z {
            // Fainter towards the ray's end, so the shadow does not stop at a line.
            shadow = max(shadow, 1.0 - t * t);
        }
    }
    return 1.0 - shadow;
}

@compute @workgroup_size(8, 8)
fn tile(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(vec2<f32>(id.xy) >= ao.size) {
        return;
    }
    let p = vec2<i32>(id.xy);
    let contact = contact_light(p);
    if ao.denoise_on <= 0.0 {
        textureStore(out_final, p, vec4<f32>(min(ao_at(p), 1.0), contact, 0.0, 0.0));
        return;
    }
    let z = depth_at(p);
    let dx = slope(depth_at(p - vec2<i32>(1, 0)), z, depth_at(p + vec2<i32>(1, 0)));
    let dy = slope(depth_at(p - vec2<i32>(0, 1)), z, depth_at(p + vec2<i32>(0, 1)));
    // XeGTAO's edge tolerance: 1.1 % of the depth per pixel of distance.
    let tolerance = 0.011 * z;
    var sum = 0.0;
    var weight = 0.0;
    for (var y = -2; y < 2; y++) {
        for (var x = -2; x < 2; x++) {
            let q = p + vec2<i32>(x, y);
            let expected = z + f32(x) * dx + f32(y) * dy;
            let off = abs(depth_at(q) - expected) / (tolerance * max(1.0, length(vec2<f32>(f32(x), f32(y)))));
            let w = clamp(1.25 - off, 0.0, 1.0);
            sum += w * ao_at(q);
            weight += w;
        }
    }
    // The centre always counts.
    let value = select(ao_at(p), sum / weight, weight > 0.0);
    textureStore(out_final, p, vec4<f32>(min(value, 1.0), contact, 0.0, 0.0));
}
