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
    // The sea (`sea.wgsl`): x on (1/0), y polar and z equatorial radius (m), w the camera's
    // height over it (m); its axis; the planet's centre (camera-relative); its water's colour.
    sea: vec4<f32>,
    sea_axis: vec4<f32>,
    sea_center: vec4<f32>,
    sea_color: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

const PI: f32 = 3.14159265;

/// Rotates v by the unit quaternion q.
fn rotate(q: vec4<f32>, v: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * cross(q.xyz, v);
    return v + q.w * t + cross(q.xyz, t);
}

// The token of a vertex drawn in its instance's own material (vertex.rs).
const NO_TOKEN: u32 = 0xffffu;
// A bevel's radius: a share of a part's smallest side, and at most (m) (primitives.rs).
const BEVEL_SHARE: f32 = 0.08;
const BEVEL_MAX: f32 = 0.025;

// Marks a part's token (vertex.rs, `PART`): a unit primitive naming a palette token.
const PART: u32 = 0x8000u;
// A part's bevel (`art-style.md` §7): a share of its smallest side, at least and at most (m),
// and at most this share of its smallest side (primitives.rs, `part_bevel_radius`).
const PART_BEVEL_SHARE: f32 = 0.02;
const PART_BEVEL_MIN: f32 = 0.005;
const PART_BEVEL_THIN: f32 = 0.2;

/// Whether a vertex's `material` is a part's (vertex.rs, `PART`).
fn is_part(material: u32) -> bool {
    let token = material & 0xffffu;
    return token != NO_TOKEN && (token & PART) != 0u;
}

/// Where a mesh vertex is drawn in its instance's space: scaled, and if it is a bevelled
/// primitive's, at its corner of the inner shape plus its normal times the bevel radius, so
/// bevels keep their size in metres (primitives.rs, `shaped`). A part's narrows along Y, its
/// top `taper` times as wide as its bottom (its instance colour's blue: `Material::part`).
fn placed(
    position: vec3<f32>, normal: vec3<f32>, material: u32, scale: vec3<f32>, taper: f32,
) -> vec3<f32> {
    let part = is_part(material);
    let bevelled = ((material & 0xffffu) == NO_TOKEN || part) && (material >> 24u) != 0u;
    let width = select(1.0, mix(1.0, taper, position.y + 0.5), part);
    let smallest = min(scale.x, min(scale.y, scale.z));
    var radius = min(BEVEL_SHARE * smallest, BEVEL_MAX);
    if part {
        radius = min(clamp(PART_BEVEL_SHARE * smallest, PART_BEVEL_MIN, BEVEL_MAX), PART_BEVEL_THIN * smallest) * width;
    }
    let inner = position * scale * vec3<f32>(width, 1.0, width);
    return inner + (normal - 2.0 * position) * select(0.0, radius, bevelled);
}

/// As `placed`, untapered.
fn shaped(position: vec3<f32>, normal: vec3<f32>, material: u32, scale: vec3<f32>) -> vec3<f32> {
    return placed(position, normal, material, scale, 1.0);
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
