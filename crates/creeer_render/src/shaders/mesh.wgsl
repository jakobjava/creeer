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

const PI: f32 = 3.14159265;
// Fraction of each cascade (at its far end) that blends into the next one.
const CASCADE_BLEND: f32 = 0.15;

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

/// Shadow lookup in one cascade: 1 = lit, 0 = shadowed.
fn cascade_visibility(world: vec3<f32>, normal: vec3<f32>, cascade: u32) -> f32 {
    // Normal offset: sample slightly above the surface to avoid self-shadowing ("acne").
    let offset = normal * globals.cascade_texel[cascade];
    let clip = globals.cascades[cascade] * vec4<f32>(world + offset, 1.0);
    let uv = vec2<f32>(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || clip.z > 1.0 {
        return 1.0;
    }
    // Four bilinear comparison taps at ±½ texel: a smooth 3×3-texel tent filter. Smaller
    // than 9 taps at ±1 texel (sharper), equally free of stair-stepping.
    let half_texel = 0.5 / globals.params.y;
    var lit = 0.0;
    for (var i = 0u; i < 4u; i++) {
        let corner = vec2<f32>(f32(i & 1u), f32(i >> 1u)) * 2.0 - 1.0;
        let tap = uv + corner * half_texel;
        lit += textureSampleCompareLevel(shadow_map, shadow_sampler, tap, cascade, clip.z);
    }
    return lit * 0.25;
}

/// Fraction of sunlight reaching `world` (0 = fully shadowed).
fn sun_visibility(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    let view_depth = dot(world, globals.camera_forward.xyz);
    let shadow_distance = globals.camera_forward.w;
    if view_depth >= shadow_distance {
        return 1.0;
    }
    var cascade = 3u;
    for (var c = 0u; c < 4u; c++) {
        if view_depth < globals.cascade_far[c] {
            cascade = c;
            break;
        }
    }
    var lit = cascade_visibility(world, normal, cascade);
    // Blend into the next cascade over the last part of this one, so the resolution change
    // is a gradual transition instead of a line that sweeps along as the camera moves.
    let far = globals.cascade_far[cascade];
    let near = select(0.0, globals.cascade_far[max(cascade, 1u) - 1u], cascade > 0u);
    let blend_start = far - CASCADE_BLEND * (far - near);
    if cascade < 3u && view_depth > blend_start {
        let next = cascade_visibility(world, normal, cascade + 1u);
        lit = mix(lit, next, smoothstep(blend_start, far, view_depth));
    }
    // Fade out over the last 10 % of the shadow distance instead of a hard edge.
    let fade = clamp((shadow_distance - view_depth) / (0.1 * shadow_distance), 0.0, 1.0);
    return mix(1.0, lit, fade);
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

    // Cook-Torrance specular: GGX distribution, Smith-Schlick geometry, Schlick Fresnel.
    let h = normalize(l + v);
    let ndl = max(dot(n, l), 0.0);
    let ndv = max(dot(n, v), 1e-4);
    let ndh = max(dot(n, h), 0.0);
    let vdh = max(dot(v, h), 0.0);
    let f0 = mix(vec3<f32>(0.04), base, metal);
    let fresnel = f0 + (1.0 - f0) * pow(1.0 - vdh, 5.0);
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = ndh * ndh * (a2 - 1.0) + 1.0;
    let distribution = a2 / (PI * denom * denom);
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    let geometry = (ndl / (ndl * (1.0 - k) + k)) * (ndv / (ndv * (1.0 - k) + k));
    let specular = distribution * geometry * fresnel / max(4.0 * ndl * ndv, 1e-4);
    let diffuse = (1.0 - fresnel) * (1.0 - metal) * base / PI;
    var visibility = 0.0;
    if ndl > 0.0 {
        visibility = sun_visibility(in.world, n);
    }
    let direct = (diffuse + specular) * globals.sun_color.rgb * ndl * visibility;

    // Hemispheric ambient: diffuse from the normal, a rough "reflection" from the mirror ray.
    let sky_amount = dot(n, up) * 0.5 + 0.5;
    let ambient_diffuse = mix(globals.ground_ambient.rgb, globals.sky_ambient.rgb, sky_amount);
    let r = reflect(-v, n);
    let reflection = mix(globals.ground_ambient.rgb, globals.sky_ambient.rgb, dot(r, up) * 0.5 + 0.5);
    let ambient_fresnel = f0 + (max(vec3<f32>(1.0 - roughness), f0) - f0) * pow(1.0 - ndv, 5.0);
    let ambient = ambient_diffuse * base * (1.0 - metal) + reflection * ambient_fresnel;

    return vec4<f32>(direct + ambient, 1.0);
}
