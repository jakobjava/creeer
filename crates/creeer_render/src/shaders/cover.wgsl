// Decoratives on the ground (G2.11, ADR-0040; `cover.rs`): grass tufts and grit, instanced,
// placed by the client on the drawn ground. Lit as the parts are (sun with its shadows and the
// contact shadows, sky and ground ambient with the screen's occlusion); they cast no shadow of
// their own into the shadow maps (the contact shadows give theirs).

@group(1) @binding(0) var shadow_map: texture_depth_2d_array;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

struct CoverIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) along: f32,
};

struct InstanceIn {
    @location(3) root: vec3<f32>,
    // x height, y width (m).
    @location(4) size: vec2<f32>,
    // xyz the ground's normal, w its turn about it (a half turn per unit).
    @location(5) up: vec4<f32>,
    // rgb sRGB colour, a tone.
    @location(6) colour: vec4<f32>,
    @location(7) lean: f32,
};

struct CoverOut {
    // Invariant: the prepass computes the same depth (`after_prepass_depth_state`).
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) colour: vec3<f32>,
    @location(3) along: f32,
};

fn cover_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@vertex
fn vs_main(v: CoverIn, i: InstanceIn) -> CoverOut {
    let up = normalize(i.up.xyz);
    let reference = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(up.x) > 0.9);
    let t0 = normalize(cross(up, reference));
    let t1 = cross(up, t0);
    let turn = i.up.w * PI;
    let e = t0 * cos(turn) + t1 * sin(turn);
    let f = cross(up, e);
    // Blades lean out more towards their tips; a pebble's `along` keeps it whole.
    let out = vec2<f32>(v.position.x, v.position.z) * (1.0 + i.lean * v.along * v.along * 1.5);
    let local = e * out.x * i.size.y + f * out.y * i.size.y + up * v.position.y * i.size.x;
    let world = i.root + local;
    var o: CoverOut;
    o.clip = globals.view_proj * vec4<f32>(world, 1.0);
    o.world = world;
    o.normal = normalize(e * v.normal.x + f * v.normal.z + up * v.normal.y);
    o.colour = cover_linear(i.colour.rgb) * (0.9 + 0.2 * i.colour.a);
    o.along = v.along;
    return o;
}

@fragment
fn fs_main(in: CoverOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let v = normalize(-in.world);
    let l = globals.sun_direction.xyz;
    // Darker at the root, where the tuft shades itself.
    let base = in.colour * (0.6 + 0.4 * in.along);
    var visibility = 0.0;
    if dot(n, l) > 0.0 {
        visibility = sun_visibility(in.world, n);
    }
    let direct = direct_light(n, v, l, base, 0.0, DIELECTRIC_F0, 0.9, globals.sun_color.rgb, visibility);
    let occlusion = ambient_occlusion(in.clip.xy, base, 1.0);
    let ambient = occlusion * ambient_light(
        n, v, globals.up.xyz, base, 0.0, DIELECTRIC_F0, 0.9, globals.sky_ambient.rgb,
        globals.ground_ambient.rgb,
    );
    return vec4<f32>(direct + ambient, 1.0);
}
