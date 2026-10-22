// Instanced meshes with "PBR-lite" shading (ADR-0003): GGX specular + Lambert diffuse from
// the sun with cascaded shadows, hemispheric sky/ground ambient. Positions are
// camera-relative (camera at origin). Two pipelines (G1.3c, ADR-0026): procedural meshes
// (vs_plain, fs_plain) take their instance's material; models (vs_main, fs_main) name palette
// tokens, carry their bake, and are drawn with surface.wgsl's full surface. Neither pays for
// what the other needs.

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
    @location(4) scale: vec3<f32>,
    // Bits 0–7 roughness, 8–15 wear from use, 16–31 years weathered (a half float).
    @location(7) surface: u32,
    @location(5) color: vec4<f32>,
};

struct PlainOut {
    // Invariant: the prepass computes the same depth (`after_prepass_depth_state`).
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    // Centroid: a triangle thinner than a pixel (a bevel) is shaded within itself, not at
    // the pixel's centre with its normal extrapolated beyond the faces'.
    @location(1) @interpolate(perspective, centroid) normal: vec3<f32>,
    // Position in the mesh's own space (m, scaled), for its surface.
    @location(2) local: vec3<f32>,
    @location(3) @interpolate(flat) color: vec3<f32>,
    // x: metalness, y: roughness
    @location(4) @interpolate(flat) material: vec2<f32>,
    // Engineering overlay value (or a huge negative number for "none").
    @location(5) @interpolate(flat) heat: f32,
};

struct ModelOut {
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) @interpolate(perspective, centroid) normal: vec3<f32>,
    @location(2) local: vec3<f32>,
    @location(3) @interpolate(flat) color: vec3<f32>,
    @location(4) @interpolate(flat) material: vec2<f32>,
    @location(5) @interpolate(flat) heat: f32,
    @location(6) @interpolate(flat) token: u32,
    // x: baked occlusion, y: edge wear, z: the part's tone, w: height above the base (0–1).
    @location(7) masks: vec4<f32>,
    // The normal in the model's own space.
    @location(8) local_normal: vec3<f32>,
    @location(9) grain: vec2<f32>,
    // x: years weathered, y: wear from use.
    @location(10) @interpolate(flat) ageing: vec2<f32>,
};

/// An instance's metalness and roughness.
fn instance_material(i: InstanceIn) -> vec2<f32> {
    return vec2<f32>(i.position_metal.w, f32(i.surface & 0xffu) / 255.0);
}

@vertex
fn vs_plain(v: VertexIn, i: InstanceIn) -> PlainOut {
    let local = shaped(v.position, v.normal, v.material, i.scale);
    let world = rotate(i.rotation, local) + i.position_metal.xyz;
    var out: PlainOut;
    out.clip = globals.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    // Inverse-transpose for non-uniform scale: divide by scale, then rotate.
    out.normal = rotate(i.rotation, v.normal / i.scale);
    out.local = local;
    out.color = i.color.rgb;
    out.material = instance_material(i);
    out.heat = i.color.a;
    return out;
}

@vertex
fn vs_main(v: VertexIn, i: InstanceIn) -> ModelOut {
    let local = v.position * i.scale;
    let world = rotate(i.rotation, local) + i.position_metal.xyz;
    var out: ModelOut;
    out.clip = globals.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = rotate(i.rotation, v.normal / i.scale);
    out.local = local;
    out.color = i.color.rgb;
    out.material = instance_material(i);
    out.heat = i.color.a;
    out.token = v.material & 0xffffu;
    out.masks = vec4<f32>(
        f32((v.material >> 16u) & 0xffu) / 255.0,
        f32(v.material >> 24u) / 255.0,
        f32(v.detail & 0xffu) / 255.0,
        f32(v.detail >> 16u) / 65535.0,
    );
    out.local_normal = normalize(v.normal / i.scale);
    out.grain = v.grain;
    out.ageing = vec2<f32>(unpack2x16float(i.surface).y, f32((i.surface >> 8u) & 0xffu) / 255.0);
    return out;
}

// Geometric specular anti-aliasing (Kaplanyan et al. 2016; Tokuyoshi and Kaplanyan 2019, as
// Unity's HDRP and Filament use it): a pixel is a Gaussian of σ = ½ pixel, so the spread of
// normals within it is a quarter of their squared change across it, and widens the lobe
// (α² grows by twice that), up to a cap. Without it, a bevel thinner than a pixel sparkles.
const PIXEL_VARIANCE: f32 = 0.25;
const SPREAD_CAP: f32 = 0.04;

/// What a pixel's spread of normals adds to α² (α = roughness²).
fn normal_spread(normal: vec3<f32>) -> f32 {
    let n = normalize(normal);
    let du = dpdx(n);
    let dv = dpdy(n);
    return min(2.0 * PIXEL_VARIANCE * (dot(du, du) + dot(dv, dv)), SPREAD_CAP);
}

@fragment
fn fs_plain(in: PlainOut) -> @location(0) vec4<f32> {
    // Derivatives first: they need uniform control flow.
    let footprint = max(length(dpdx(in.local)), length(dpdy(in.local)));
    let spread = normal_spread(in.normal);
    // Metalness from 2 up marks a light source emitting (w − 2) times its colour.
    if in.material.x >= 2.0 {
        return vec4<f32>(in.color * (in.material.x - 2.0), 1.0);
    }
    let at = Place(in.local, vec3<f32>(0.0, 1.0, 0.0), vec2<f32>(0.0), footprint, vec2<f32>(0.0));
    let surface = plain_surface(in.color, in.material.y, in.material.x, at);
    let occlusion = ambient_occlusion(in.clip.xy, surface.colour, 1.0);
    return vec4<f32>(lit(in.world, in.normal, spread, surface, in.heat, occlusion), 1.0);
}

@fragment
fn fs_main(in: ModelOut) -> @location(0) vec4<f32> {
    return shade_model(in);
}

/// A model's fragment, shaded (entry points cannot call one another).
fn shade_model(in: ModelOut) -> vec4<f32> {
    let footprint = max(length(dpdx(in.local)), length(dpdy(in.local)));
    let grain_footprint = max(abs(dpdx(in.grain)), abs(dpdy(in.grain)));
    let spread = normal_spread(in.normal);
    if in.material.x >= 2.0 {
        return vec4<f32>(in.color * (in.material.x - 2.0), 1.0);
    }
    let at = Place(in.local, normalize(in.local_normal), in.grain, footprint, grain_footprint);
    let surface = model_surface(
        in.token, in.color, in.masks.x, in.masks.y, in.masks.z, in.masks.w,
        in.ageing.x, in.ageing.y, at,
    );
    let occlusion = ambient_occlusion(in.clip.xy, surface.colour, in.masks.x);
    return vec4<f32>(lit(in.world, in.normal, spread, surface, in.heat, occlusion), 1.0);
}

/// A model drawn two-sided (G1.3e: the engineer's garments are single shells): a back face
/// seen, the inside of a collar or a sleeve, is lit as a face turned to the eye (glTF's
/// `doubleSided`).
@fragment
fn fs_two_sided(in: ModelOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var turned = in;
    if !front {
        turned.normal = -in.normal;
        turned.local_normal = -in.local_normal;
    }
    return shade_model(turned);
}

/// `surface` at `world` lit by the sun (with its shadows) and the sky; `spread` (from
/// `normal_spread`) widens its highlights, `occlusion` (baked and on screen:
/// `ambient_occlusion`) shades the ambient light only. With the engineering overlay on, `heat`
/// replaces the material.
fn lit(
    world: vec3<f32>, normal: vec3<f32>, spread: f32, surface: Surface, heat: f32, occlusion: vec3<f32>,
) -> vec3<f32> {
    let n = normalize(normal);
    let v = normalize(-world);
    let l = globals.sun_direction.xyz;
    var base = surface.colour;
    var metal = surface.metalness;
    // Its own roughness r, widened by the pixel's spread of normals: α² + spread, α = r².
    let own = clamp(surface.roughness, 0.045, 1.0);
    var roughness = sqrt(sqrt(min(own * own * own * own + spread, 1.0)));
    if globals.heatmap.x > 0.5 {
        if heat > -1.0e37 {
            // Overlay: the measured value replaces the material; keep a matte finish so
            // reflections cannot be mistaken for data.
            base = heatmap_color(heat);
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
        visibility = sun_visibility(world, n);
    }
    let direct = direct_light(n, v, l, base, metal, roughness, globals.sun_color.rgb, visibility);
    let ambient = occlusion * ambient_light(
        n, v, globals.up.xyz, base, metal, roughness, globals.sky_ambient.rgb,
        globals.ground_ambient.rgb,
    );
    return direct + ambient;
}
