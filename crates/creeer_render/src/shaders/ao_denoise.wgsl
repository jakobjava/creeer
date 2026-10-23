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

@compute @workgroup_size(8, 8)
fn tile(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(vec2<f32>(id.xy) >= ao.size) {
        return;
    }
    let p = vec2<i32>(id.xy);
    if ao.denoise_on <= 0.0 {
        textureStore(out_final, p, vec4<f32>(min(ao_at(p), 1.0)));
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
    textureStore(out_final, p, vec4<f32>(min(value, 1.0)));
}
