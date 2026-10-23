// Lighting shared by the mesh and terrain passes: sun visibility from the cascaded shadow
// maps, Cook-Torrance/Lambert direct light and hemispheric ambient light ("PBR-lite",
// ADR-0003). The including shader declares `shadow_map` and `shadow_sampler`.

// A non-metal's reflectance at normal incidence (index about 1.5); skin's is lower (surface.wgsl).
const DIELECTRIC_F0: f32 = 0.04;

// Fraction of each cascade (at its far end) that blends into the next one.
const CASCADE_BLEND: f32 = 0.15;

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


/// Sunlight reflected towards the viewer: Cook-Torrance specular (GGX distribution,
/// Smith-Schlick geometry, Schlick Fresnel) plus Lambert diffuse, for sun irradiance
/// `irradiance` (on a surface facing the sun) and shadowing `visibility`; `dielectric` is the
/// non-metal's reflectance at normal incidence.
fn direct_light(
    n: vec3<f32>, v: vec3<f32>, l: vec3<f32>,
    base: vec3<f32>, metal: f32, dielectric: f32, roughness: f32,
    irradiance: vec3<f32>, visibility: f32,
) -> vec3<f32> {
    let h = normalize(l + v);
    let ndl = max(dot(n, l), 0.0);
    let ndv = max(dot(n, v), 1e-4);
    let ndh = max(dot(n, h), 0.0);
    let vdh = max(dot(v, h), 0.0);
    let f0 = mix(vec3<f32>(dielectric), base, metal);
    let fresnel = f0 + (1.0 - f0) * pow(1.0 - vdh, 5.0);
    let a = roughness * roughness;
    let a2 = a * a;
    let denom = ndh * ndh * (a2 - 1.0) + 1.0;
    let distribution = a2 / (PI * denom * denom);
    let k = (roughness + 1.0) * (roughness + 1.0) / 8.0;
    let geometry = (ndl / (ndl * (1.0 - k) + k)) * (ndv / (ndv * (1.0 - k) + k));
    let specular = distribution * geometry * fresnel / max(4.0 * ndl * ndv, 1e-4);
    let diffuse = (1.0 - fresnel) * (1.0 - metal) * base / PI;
    return (diffuse + specular) * irradiance * ndl * visibility;
}

/// Hemispheric ambient light: diffuse from the normal, a rough "reflection" from the mirror
/// ray, between the ground's radiance `ground` and the sky's `sky` about `up`.
fn ambient_light(
    n: vec3<f32>, v: vec3<f32>, up: vec3<f32>,
    base: vec3<f32>, metal: f32, dielectric: f32, roughness: f32,
    sky: vec3<f32>, ground: vec3<f32>,
) -> vec3<f32> {
    let ndv = max(dot(n, v), 1e-4);
    let f0 = mix(vec3<f32>(dielectric), base, metal);
    let sky_amount = dot(n, up) * 0.5 + 0.5;
    let ambient_diffuse = mix(ground, sky, sky_amount);
    let r = reflect(-v, n);
    let reflection = mix(ground, sky, dot(r, up) * 0.5 + 0.5);
    let ambient_fresnel = f0 + (max(vec3<f32>(1.0 - roughness), f0) - f0) * pow(1.0 - ndv, 5.0);
    return ambient_diffuse * base * (1.0 - metal) + reflection * ambient_fresnel;
}
