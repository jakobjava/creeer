// Instanced meshes with "PBR-lite" shading (ADR-0003): GGX specular + Lambert diffuse from
// the sun with cascaded shadows, hemispheric sky/ground ambient. Positions are
// camera-relative (camera at origin).

@group(1) @binding(0) var shadow_map: texture_depth_2d_array;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
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
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let v = normalize(-in.world);
    let l = globals.sun_direction.xyz;
    let up = globals.up.xyz;
    var base = in.color;
    var metal = in.material.x;
    var roughness = clamp(in.material.y, 0.045, 1.0);
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
    let ambient = ambient_light(
        n, v, up, base, metal, roughness, globals.sky_ambient.rgb, globals.ground_ambient.rgb,
    );
    return vec4<f32>(direct + ambient, 1.0);
}
