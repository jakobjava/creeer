// Per-frame constants shared by all scene passes (must match `Globals` in renderer.rs).
struct Globals {
    view_proj: mat4x4<f32>,
    // xyz: unit vector towards the sun (camera-relative space = world orientation).
    sun_direction: vec4<f32>,
    // rgb: sun colour × intensity (linear).
    sun_color: vec4<f32>,
    sky_ambient: vec4<f32>,
    ground_ambient: vec4<f32>,
    // xyz: local "up" (away from the planet centre).
    up: vec4<f32>,
    // x: exposure, y: shadow map size (texels).
    params: vec4<f32>,
    // xyz: camera forward, w: shadow distance (m).
    camera_forward: vec4<f32>,
    // Sun shadow cascades: camera-relative position → shadow clip space.
    cascades: array<mat4x4<f32>, 4>,
    // Far end of each cascade along the view direction (m).
    cascade_far: vec4<f32>,
    // World size of one shadow-map texel per cascade (m).
    cascade_texel: vec4<f32>,
    // Camera-relative clip space → camera-relative world (for sky view rays).
    inv_view_proj: mat4x4<f32>,
    // Atmosphere: rgb Rayleigh scattering (1/m), w Rayleigh scale height (m).
    atmo_rayleigh: vec4<f32>,
    // x Mie scattering, y Mie extinction (1/m), z Mie scale height (m), w Mie g.
    atmo_mie: vec4<f32>,
    // x planet radius, y atmosphere height, z camera height above ground (m), w sky on (1/0).
    atmo_geometry: vec4<f32>,
    // rgb sun irradiance at the top of the atmosphere, w star brightness.
    sun_irradiance: vec4<f32>,
    // rgb ground albedo.
    ground_albedo: vec4<f32>,
    // Engineering overlay: x on (1/0), y value at scale bottom, z value at top, w palette.
    heatmap: vec4<f32>,
    // Cosine-weighted sky radiance at the ground for sun cosines from −0.2 to 1 (16 steps).
    sky_irradiance: array<vec4<f32>, 16>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

const PI: f32 = 3.14159265;

/// Rotates v by the unit quaternion q.
fn rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

/// How far a terrain vertex at `position` (chunk-local) blends towards the next coarser level
/// of detail (0..1): from 4.25 to 5 chunk sizes (`origin.w`) from the camera, where a coarser
/// neighbour may begin (geomorphing, creeer_terrain::levels and stream_tests).
fn coarseness(position: vec3<f32>, origin: vec4<f32>, rotation: vec4<f32>) -> f32 {
    let size = origin.w;
    if size <= 0.0 {
        return 0.0;
    }
    let distance = length(rotate(rotation, position) + origin.xyz);
    return smoothstep(4.25 * size, 5.0 * size, distance);
}
