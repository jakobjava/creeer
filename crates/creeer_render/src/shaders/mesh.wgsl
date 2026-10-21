// Instanced meshes with "PBR-lite" shading (ADR-0003): GGX specular + Lambert diffuse from
// the sun with cascaded shadows, hemispheric sky/ground ambient. Positions are
// camera-relative (camera at origin). Model vertices name palette tokens (G1.3b, ADR-0026);
// procedural ones take their instance's material.

@group(1) @binding(0) var shadow_map: texture_depth_2d_array;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

// One palette token (renderer/palette.rs, `TokenRaw`).
struct Token {
    colour: vec3<f32>,
    roughness: f32,
    metalness: f32,
    // 1: each player has their own colour, the instance's.
    per_player: f32,
    wears_to: u32,
    pad: u32,
};
@group(2) @binding(0) var<storage, read> palette: array<Token>;

// A vertex drawn in its instance's own material.
const NO_TOKEN: u32 = 0xffffu;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Bits 0–15 the token, 16–23 baked occlusion, 24–31 edge wear.
    @location(6) material: u32,
};

struct InstanceIn {
    @location(2) rotation: vec4<f32>,
    // xyz: camera-relative position, w: metalness
    @location(3) position_metal: vec4<f32>,
    // xyz: scale, w: roughness
    @location(4) scale_rough: vec4<f32>,
    @location(5) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    // x: metalness, y: roughness
    @location(3) material: vec2<f32>,
    // Engineering overlay value (or a huge negative number for "none").
    @location(4) @interpolate(flat) heat: f32,
    @location(5) @interpolate(flat) token: u32,
    // x: baked occlusion, y: edge wear.
    @location(6) masks: vec2<f32>,
};

@vertex
fn vs_main(v: VertexIn, i: InstanceIn) -> VertexOut {
    let world = rotate(i.rotation, v.position * i.scale_rough.xyz) + i.position_metal.xyz;
    var out: VertexOut;
    out.clip = globals.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    // Inverse-transpose for non-uniform scale: divide by scale, then rotate.
    out.normal = rotate(i.rotation, v.normal / i.scale_rough.xyz);
    out.color = i.color.rgb;
    out.material = vec2<f32>(i.position_metal.w, i.scale_rough.w);
    out.heat = i.color.a;
    out.token = v.material & 0xffffu;
    out.masks = vec2<f32>(f32((v.material >> 16u) & 0xffu), f32(v.material >> 24u)) / 255.0;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Metalness from 2 up marks a light source emitting (w − 2) times its colour.
    if in.material.x >= 2.0 {
        return vec4<f32>(in.color * (in.material.x - 2.0), 1.0);
    }
    let n = normalize(in.normal);
    let v = normalize(-in.world);
    let l = globals.sun_direction.xyz;
    let up = globals.up.xyz;
    var base = in.color;
    var metal = in.material.x;
    var roughness = in.material.y;
    if in.token != NO_TOKEN {
        let token = palette[in.token];
        if token.per_player < 0.5 {
            base = token.colour;
        }
        metal = token.metalness;
        roughness = token.roughness;
    }
    roughness = clamp(roughness, 0.045, 1.0);
    if globals.heatmap.x > 0.5 {
        if in.heat > -1.0e37 {
            // Overlay: the measured value replaces the material; keep a matte finish so
            // reflections cannot be mistaken for data.
            base = heatmap_color(in.heat);
            metal = 0.0;
            roughness = 0.8;
        } else {
            // Parts without data recede into a dim grey.
            base = vec3<f32>(dot(base, vec3<f32>(0.2126, 0.7152, 0.0722)) * 0.5);
            roughness = 0.9;
            metal = 0.0;
        }
    }
    var visibility = 0.0;
    if dot(n, l) > 0.0 {
        visibility = sun_visibility(in.world, n);
    }
    let direct = direct_light(n, v, l, base, metal, roughness, globals.sun_color.rgb, visibility);
    // Baked occlusion shades the ambient light only: the sun has its shadows.
    let ambient = in.masks.x * ambient_light(
        n, v, up, base, metal, roughness, globals.sky_ambient.rgb, globals.ground_ambient.rgb,
    );
    return vec4<f32>(direct + ambient, 1.0);
}
