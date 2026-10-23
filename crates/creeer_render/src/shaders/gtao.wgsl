// GTAO, pass 2 (XeGTAO's MainPass, normals generated in place from the 32-bit depth): for
// each pixel, slices through the hemisphere about the view direction; in each, the highest
// horizon on either side found by stepping out over the depth MIPs, and the cosine-weighted
// visible arc between them integrated in closed form (Jimenez et al. 2016, Algorithm 1).

@group(0) @binding(1) var depths: texture_2d<f32>;
@group(0) @binding(2) var nearest: sampler;
@group(0) @binding(3) var out_ao: texture_storage_2d<r32float, write>;

const HALF_PI: f32 = 1.5707963;
// The slices' turns and the steps' offsets: 16 points of the R2 sequence (Roberts 2018),
// laid over each 4×4 tile in Bayer order so neighbours differ most; the denoise's 4×4 mean
// takes in all 16.
const BAYER = array<u32, 16>(0u, 8u, 2u, 10u, 12u, 4u, 14u, 6u, 3u, 11u, 1u, 9u, 15u, 7u, 13u, 5u);

fn base_level() -> i32 {
    return i32(ao.base_mip);
}

fn grid_depth(p: vec2<i32>) -> f32 {
    let level = base_level();
    let last = vec2<i32>(textureDimensions(depths, level)) - 1;
    return textureLoad(depths, clamp(p, vec2<i32>(0), last), level).r;
}

/// How far each neighbour (left, right, top, bottom) lies on this pixel's surface: 1 the
/// same surface, 0 an edge (XeGTAO's slope-adjusted test).
fn edges(z: f32, l: f32, r: f32, t: f32, b: f32) -> vec4<f32> {
    var e = vec4<f32>(l, r, t, b) - z;
    let slope_lr = (e.y - e.x) * 0.5;
    let slope_tb = (e.w - e.z) * 0.5;
    let adjusted = e + vec4<f32>(slope_lr, -slope_lr, slope_tb, -slope_tb);
    e = min(abs(e), abs(adjusted));
    return clamp(1.25 - e / (z * 0.011), vec4<f32>(0.0), vec4<f32>(1.0));
}

fn normal_from(e: vec4<f32>, c: vec3<f32>, l: vec3<f32>, r: vec3<f32>, t: vec3<f32>, b: vec3<f32>) -> vec3<f32> {
    let accepted = clamp(vec4<f32>(e.x * e.z, e.z * e.y, e.y * e.w, e.w * e.x) + 0.01, vec4<f32>(0.0), vec4<f32>(1.0));
    let dl = normalize(l - c);
    let dr = normalize(r - c);
    let dt = normalize(t - c);
    let db = normalize(b - c);
    return normalize(
        accepted.x * cross(dl, dt) + accepted.y * cross(dt, dr) + accepted.z * cross(dr, db)
            + accepted.w * cross(db, dl),
    );
}

/// acos for [−1, 1] (Lagarde 2014), as XeGTAO uses it.
fn fast_acos(x: f32) -> f32 {
    let a = abs(x);
    let r = (-0.156583 * a + HALF_PI) * sqrt(1.0 - a);
    return select(PI - r, r, x >= 0.0);
}


@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let pix = vec2<i32>(id.xy);
    if any(vec2<f32>(id.xy) >= ao.size) {
        return;
    }
    let screen = (vec2<f32>(id.xy) + 0.5) * ao.pixel;
    var z = grid_depth(pix);
    let zl = grid_depth(pix + vec2<i32>(-1, 0));
    let zr = grid_depth(pix + vec2<i32>(1, 0));
    let zt = grid_depth(pix + vec2<i32>(0, -1));
    let zb = grid_depth(pix + vec2<i32>(0, 1));
    let e = edges(z, zl, zr, zt, zb);
    if z >= FAR_DEPTH {
        textureStore(out_ao, pix, vec4<f32>(1.0));
        return;
    }
    let normal = normal_from(
        e,
        view_position(screen, z),
        view_position(screen + vec2<f32>(-ao.pixel.x, 0.0), zl),
        view_position(screen + vec2<f32>(ao.pixel.x, 0.0), zr),
        view_position(screen + vec2<f32>(0.0, -ao.pixel.y), zt),
        view_position(screen + vec2<f32>(0.0, ao.pixel.y), zb),
    );
    // Towards the camera by a hair, against depth rounding (XeGTAO's value for 32-bit depth).
    z *= 0.99999;
    let centre = view_position(screen, z);
    let view = normalize(-centre);

    let radius = ao.radius;
    let range = ao.falloff * radius;
    let fade_from = radius * (1.0 - ao.falloff);
    let falloff_mul = -1.0 / range;
    let falloff_add = fade_from / range + 1.0;

    let q = id.xy % 4u;
    let index = BAYER[q.y * 4u + q.x];
    let noise = fract(0.5 + f32(index) * vec2<f32>(0.75487766, 0.56984029));

    let pixel_size = z * ao.mul_x_pixel;
    let screen_radius = radius / pixel_size.x;
    // Tiny on screen: fades to unoccluded.
    var visibility = clamp((10.0 - screen_radius) / 100.0, 0.0, 1.0) * 0.5;
    // Never sample the centre pixel itself.
    let too_close = 1.3;
    let min_s = too_close / screen_radius;
    let level = base_level();

    for (var slice = 0.0; slice < ao.slices; slice += 1.0) {
        let phi = (slice + noise.x) / ao.slices * PI;
        let cos_phi = cos(phi);
        let sin_phi = sin(phi);
        let omega = vec2<f32>(cos_phi, -sin_phi) * screen_radius;
        let direction = vec3<f32>(cos_phi, sin_phi, 0.0);
        let ortho = direction - dot(direction, view) * view;
        let axis = normalize(cross(ortho, view));
        let projected = normal - axis * dot(normal, axis);
        let sign_n = sign(dot(ortho, projected));
        var projected_length = length(projected);
        let cos_n = clamp(dot(projected, view) / projected_length, 0.0, 1.0);
        let n = sign_n * fast_acos(cos_n);
        let low0 = cos(n + HALF_PI);
        let low1 = cos(n - HALF_PI);
        var horizon0 = low0;
        var horizon1 = low1;
        for (var step = 0.0; step < ao.steps; step += 1.0) {
            // R1 sequence over the steps.
            let step_noise = fract(noise.y + (slice + step * ao.steps) * 0.618034);
            var s = (step + step_noise) / ao.steps;
            s = pow(s, ao.distribution) + min_s;
            var offset = s * omega;
            let mip = clamp(log2(length(offset)) - ao.mip_offset, 0.0, f32(DEPTH_MIPS - 1));
            let lod = min(f32(level) + mip, f32(DEPTH_MIPS - 1));
            offset = round(offset) * ao.pixel;
            let at0 = screen + offset;
            let at1 = screen - offset;
            let delta0 = view_position(at0, textureSampleLevel(depths, nearest, at0, lod).r) - centre;
            let delta1 = view_position(at1, textureSampleLevel(depths, nearest, at1, lod).r) - centre;
            let dist0 = length(delta0);
            let dist1 = length(delta1);
            let w0 = clamp(dist0 * falloff_mul + falloff_add, 0.0, 1.0);
            let w1 = clamp(dist1 * falloff_mul + falloff_add, 0.0, 1.0);
            let shc0 = mix(low0, dot(delta0 / dist0, view), w0);
            let shc1 = mix(low1, dot(delta1 / dist1, view), w1);
            horizon0 = max(horizon0, shc0);
            horizon1 = max(horizon1, shc1);
        }
        // XeGTAO's correction for slight over-darkening on steep slopes.
        projected_length = mix(projected_length, 1.0, 0.05);
        let h0 = -fast_acos(horizon1);
        let h1 = fast_acos(horizon0);
        let arc0 = (cos_n + 2.0 * h0 * sin(n) - cos(2.0 * h0 - n)) / 4.0;
        let arc1 = (cos_n + 2.0 * h1 * sin(n) - cos(2.0 * h1 - n)) / 4.0;
        visibility += projected_length * (arc0 + arc1);
    }
    visibility /= ao.slices;
    visibility = max(0.03, pow(max(visibility, 0.0), ao.power));
    textureStore(out_ao, pix, vec4<f32>(visibility));
}
