// Instanced meshes with "PBR-lite" shading (ADR-0003): GGX specular + Lambert diffuse from
// the sun with cascaded shadows, hemispheric sky/ground ambient. Positions are
// camera-relative (camera at origin). Model vertices name palette tokens and carry their bake;
// their surfaces are surface.wgsl's (G1.3b–c, ADR-0026). Procedural vertices take their
// instance's material.

@group(1) @binding(0) var shadow_map: texture_depth_2d_array;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Bits 0–15 the token, 16–23 baked occlusion, 24–31 edge wear.
    @location(6) material: u32,
    // Bits 0–7 the part's tone, 16–31 the height above the model's base.
    @location(8) detail: u32,
    // Grain coordinates (m).
    @location(9) grain: vec2<f32>,
};

struct InstanceIn {
    @location(2) rotation: vec4<f32>,
    // xyz: camera-relative position, w: metalness
    @location(3) position_metal: vec4<f32>,
    // xyz: scale, w: roughness
    @location(4) scale_rough: vec4<f32>,
    @location(5) color: vec4<f32>,
    // x: years weathered, y: wear from use.
    @location(7) ageing: vec4<f32>,
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
    // x: baked occlusion, y: edge wear, z: the part's tone, w: height above the base (0–1).
    @location(6) masks: vec4<f32>,
    // Position and normal in the model's own space (m, scaled), for its surface.
    @location(7) local: vec3<f32>,
    @location(8) local_normal: vec3<f32>,
    @location(9) grain: vec2<f32>,
    // x: years weathered, y: wear from use.
    @location(10) @interpolate(flat) ageing: vec2<f32>,
};

@vertex
fn vs_main(v: VertexIn, i: InstanceIn) -> VertexOut {
    let local = v.position * i.scale_rough.xyz;
    let world = rotate(i.rotation, local) + i.position_metal.xyz;
    var out: VertexOut;
    out.clip = globals.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    // Inverse-transpose for non-uniform scale: divide by scale, then rotate.
    out.normal = rotate(i.rotation, v.normal / i.scale_rough.xyz);
    out.color = i.color.rgb;
    out.material = vec2<f32>(i.position_metal.w, i.scale_rough.w);
    out.heat = i.color.a;
    out.token = v.material & 0xffffu;
    out.masks = vec4<f32>(
        f32((v.material >> 16u) & 0xffu) / 255.0,
        f32(v.material >> 24u) / 255.0,
        f32(v.detail & 0xffu) / 255.0,
        f32(v.detail >> 16u) / 65535.0,
    );
    out.local = local;
    out.local_normal = normalize(v.normal / i.scale_rough.xyz);
    out.grain = v.grain;
    out.ageing = i.ageing.xy;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Footprints first: derivatives need uniform control flow.
    let footprint = max(length(dpdx(in.local)), length(dpdy(in.local)));
    let grain_footprint = max(abs(dpdx(in.grain)), abs(dpdy(in.grain)));
    // Metalness from 2 up marks a light source emitting (w − 2) times its colour.
    if in.material.x >= 2.0 {
        return vec4<f32>(in.color * (in.material.x - 2.0), 1.0);
    }
    let n = normalize(in.normal);
    let v = normalize(-in.world);
    let l = globals.sun_direction.xyz;
    let up = globals.up.xyz;
    let at = Place(in.local, normalize(in.local_normal), in.grain, footprint, grain_footprint);
    var surface: Surface;
    if in.token != NO_TOKEN {
        surface = model_surface(
            in.token, in.color, in.masks.x, in.masks.y, in.masks.z, in.masks.w,
            in.ageing.x, in.ageing.y, at,
        );
    } else {
        surface = plain_surface(in.color, in.material.y, in.material.x, at);
    }
    var base = surface.colour;
    var metal = surface.metalness;
    var roughness = clamp(surface.roughness, 0.045, 1.0);
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
