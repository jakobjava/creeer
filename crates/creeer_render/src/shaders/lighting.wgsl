// Lighting shared by the mesh and terrain passes: sun visibility from the cascaded shadow
// maps, Cook-Torrance/Lambert direct light and hemispheric ambient light ("PBR-lite",
// ADR-0003). The including shader declares `shadow_map` and `shadow_sampler`, and includes
// `ao_sample.wgsl` (contact shadows).

// A non-metal's reflectance at normal incidence (index about 1.5); skin's is lower (surface.wgsl).
const DIELECTRIC_F0: f32 = 0.04;

// Fraction of each cascade (at its far end) that blends into the next one.
const CASCADE_BLEND: f32 = 0.15;

/// Where the taps of Castaño's 7×7 filter fall along one axis (texels from the base), and
/// their weights (summing to −52), for a point a fraction `s` into its texel ("Shadow Mapping
/// Summary", The Witness, 2013; `shadow_filter.rs` mirrors and tests it).
fn castano7(s: f32) -> array<vec2<f32>, 4> {
    let w0 = 5.0 * s - 6.0;
    let w1 = 11.0 * s - 28.0;
    let w2 = -(11.0 * s + 17.0);
    let w3 = -(5.0 * s + 1.0);
    return array<vec2<f32>, 4>(
        vec2<f32>((4.0 * s - 5.0) / w0 - 3.0, w0),
        vec2<f32>((4.0 * s - 16.0) / w1 - 1.0, w1),
        vec2<f32>(-(7.0 * s + 5.0) / w2 + 1.0, w2),
        vec2<f32>(-s / w3 + 3.0, w3),
    );
}

/// How the receiving surface's shadow-map depth changes per texel along the map's u and v
/// (its plane, from its normal): each tap of a wide filter compares at the surface's own depth
/// there, not at its centre's, so a slope does not shadow itself ("receiver plane depth bias",
/// Isidoro, GDC 2006; here from the normal, which needs no screen derivatives).
fn receiver_slope(normal: vec3<f32>, cascade: u32) -> vec2<f32> {
    let m = globals.cascades[cascade];
    // Rows of the cascade's projection: right / radius, up / radius, −to_sun / range.
    let right = normalize(vec3<f32>(m[0].x, m[1].x, m[2].x));
    let up = normalize(vec3<f32>(m[0].y, m[1].y, m[2].y));
    let range = 1.0 / length(vec3<f32>(m[0].z, m[1].z, m[2].z));
    let towards = max(dot(normal, globals.sun_direction.xyz), 0.2);
    let per_texel = globals.cascade_texel[cascade] / (towards * range);
    // v runs down the map, against `up`.
    return vec2<f32>(dot(normal, right), -dot(normal, up)) * per_texel;
}

/// Shadow lookup in one cascade: 1 = lit, 0 = shadowed.
///
/// Sixteen bilinear comparison taps weighted as Castaño's 7×7 filter: a smooth bell about seven
/// texels wide, continuous as the point moves across texels, so a shadow's edge never shows
/// the shadow map's texels as stairs (the 3×3 tent before it did, G2.11). Each tap compares at
/// the receiving surface's depth under it (`receiver_slope`), so the normal offset can stay
/// small and contacts stay dark.
fn cascade_visibility(world: vec3<f32>, normal: vec3<f32>, cascade: u32) -> f32 {
    // Normal offset: a texel above the surface, against self-shadowing ("acne").
    let texel = globals.cascade_texel[cascade];
    let clip = globals.cascades[cascade] * vec4<f32>(world + normal * texel, 1.0);
    let uv = vec2<f32>(clip.x * 0.5 + 0.5, 0.5 - clip.y * 0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || clip.z > 1.0 {
        return 1.0;
    }
    let slope = receiver_slope(normal, cascade);
    let size = globals.params.y;
    let at = uv * size;
    let base = floor(at + 0.5);
    let st = at + 0.5 - base;
    let origin = base - 0.5;
    var us = castano7(st.x);
    var vs = castano7(st.y);
    var lit = 0.0;
    for (var j = 0u; j < 4u; j++) {
        for (var i = 0u; i < 4u; i++) {
            let offset = vec2<f32>(us[i].x, vs[j].x);
            let tap = (origin + offset) / size;
            // The receiving surface's own depth under the tap.
            let depth = clip.z + dot(offset - st, slope);
            let weight = us[i].y * vs[j].y;
            lit += weight * textureSampleCompareLevel(shadow_map, shadow_sampler, tap, cascade, depth);
        }
    }
    return lit / 2704.0;
}

/// Fraction of sunlight reaching `world` (0 = fully shadowed): the shadow maps', and the
/// screen's contact shadows where they are darker (`ao_sample.wgsl`).
fn sun_visibility(world: vec3<f32>, normal: vec3<f32>) -> f32 {
    return min(map_visibility(world, normal), contact_light_at(world));
}

/// Fraction of sunlight reaching `world` by the cascaded shadow maps.
fn map_visibility(world: vec3<f32>, normal: vec3<f32>) -> f32 {
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
