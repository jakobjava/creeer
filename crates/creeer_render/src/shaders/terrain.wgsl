// Terrain chunks (ADR-0006): the rock's colour per vertex and the ground's layers over it, with
// procedural detail and a fine relief (`ground.wgsl`, ADR-0037), lit by the sun as it reaches
// each point through the atmosphere (so the night side is dark and the terminator glows), by the
// sky for the sun's elevation there, with sun shadows and baked sky occlusion, and seen through
// the air between it and the camera (aerial perspective). Positions are camera-relative.

@group(1) @binding(0) var shadow_map: texture_depth_2d_array;
@group(1) @binding(1) var shadow_sampler: sampler_comparison;
@group(2) @binding(0) var transmittance_lut: texture_2d<f32>;
@group(2) @binding(1) var multiscatter_lut: texture_2d<f32>;
@group(2) @binding(2) var lut_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    // xyz: unit normal, w: openness to the sky (snorm).
    @location(1) normal: vec4<f32>,
    // rgb: sRGB base colour, a: roughness.
    @location(2) color: vec4<f32>,
    // xyz: unit normal of the next coarser level of detail.
    @location(3) coarse_normal: vec4<f32>,
    // Distance along `normal` to the next coarser level's surface (m).
    @location(4) morph: f32,
    // The share of the ground each of its four layers covers here.
    @location(8) cover: vec4<f32>,
};

struct InstanceIn {
    // xyz: camera-relative chunk origin, w: chunk size (0: no blending).
    @location(5) origin: vec4<f32>,
    // The body's rotation (body-fixed → world), a quaternion.
    @location(6) rotation: vec4<f32>,
    // xyz: the chunk origin within the ground's pattern (body-fixed, modulo PATTERN_PERIOD).
    @location(7) pattern: vec4<f32>,
};

struct VertexOut {
    // Invariant: the prepass computes the same depth (`after_prepass_depth_state`).
    @builtin(position) @invariant clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Where the point lies in the ground's pattern: body-fixed, modulo PATTERN_PERIOD (each
    // chunk is told its origin's place in it), so the pattern is fixed to the ground and
    // continuous across chunks of every size.
    @location(2) local: vec3<f32>,
    @location(3) color: vec3<f32>,
    // x: roughness, y: openness.
    @location(4) material: vec2<f32>,
    @location(5) cover: vec4<f32>,
};

// Steps of the aerial-perspective ray march.
const AIR_STEPS: u32 = 10u;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let low = c / 12.92;
    let high = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(high, low, c <= vec3<f32>(0.04045));
}

@vertex
fn vs_main(v: VertexIn, i: InstanceIn) -> VertexOut {
    let m = coarseness(v.position, i.origin, i.rotation);
    let world = rotate(i.rotation, v.position + v.normal.xyz * (v.morph * m)) + i.origin.xyz;
    var out: VertexOut;
    out.clip = globals.view_proj * vec4<f32>(world, 1.0);
    out.world = world;
    out.normal = rotate(i.rotation, normalize(mix(v.normal.xyz, v.coarse_normal.xyz, m)));
    out.local = v.position + i.pattern.xyz;
    out.color = srgb_to_linear(v.color.rgb);
    out.material = vec2<f32>(v.color.a, clamp(v.normal.w, 0.0, 1.0));
    out.cover = v.cover;
    return out;
}

/// Cosine-weighted sky radiance for a sun cosine `mu_s` against the local vertical
/// (table computed on the CPU from the atmosphere, `globals.sky_irradiance`).
fn sky_radiance(mu_s: f32) -> vec3<f32> {
    let x = clamp((mu_s + 0.2) / 1.2, 0.0, 1.0) * 15.0;
    let i = min(u32(floor(x)), 14u);
    return mix(globals.sky_irradiance[i].rgb, globals.sky_irradiance[i + 1u].rgb, x - f32(i));
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let smooth_normal = normalize(in.normal);
    let footprint = max(length(fwidth(in.local)), 1e-4);
    let ground = ground_at(in.local, in.color, in.cover, footprint);
    // The fine relief turns the normal the light sees (not the shadows' offset).
    let n = relief(smooth_normal, in.world, ground.height);
    let v = normalize(-in.world);
    let l = globals.sun_direction.xyz;
    let roughness = clamp(in.material.x, 0.045, 1.0);
    let openness = in.material.y;
    let base = ground.colour;

    var sun = globals.sun_color.rgb;
    var sky = globals.sky_ambient.rgb;
    var up = globals.up.xyz;
    let sky_on = globals.atmo_geometry.w > 0.5;
    let t = length(in.world);
    let dir = in.world / max(t, 1e-6);
    let h0 = globals.atmo_geometry.z;
    let mu = dot(dir, globals.up.xyz);
    let nu = dot(dir, l);
    let mu_s0 = dot(globals.up.xyz, l);
    if sky_on {
        // Height and sun elevation at this point, cancellation-free (as the sky pass does).
        let r0 = planet_radius() + h0;
        let h = height_at(h0, mu, t);
        let mu_s = (r0 * mu_s0 + t * nu) / (planet_radius() + h);
        sun = globals.sun_irradiance.rgb * transmittance_to_space(max(h, 0.0), mu_s);
        sky = sky_radiance(mu_s);
        up = normalize(globals.up.xyz * r0 + in.world);
    }
    var visibility = 0.0;
    if dot(smooth_normal, l) > 0.0 {
        visibility = sun_visibility(in.world, smooth_normal);
    }
    let direct = direct_light(n, v, l, base, 0.0, DIELECTRIC_F0, roughness, sun, visibility);
    // Light bounced from the surroundings: the planet's albedo under sun and sky.
    let bounce = globals.ground_albedo.rgb / PI * (sun * max(dot(up, l), 0.0) + sky * PI);
    let occlusion = ambient_occlusion(in.clip.xy, base, 1.0);
    let ambient = ambient_light(n, v, up, base, 0.0, DIELECTRIC_F0, roughness, sky, bounce) * openness * occlusion;
    var color = direct + ambient;

    if sky_on {
        let span = atmosphere_span(h0, mu);
        if span.y > 0.0 {
            let air = inscatter(h0, mu, mu_s0, nu, span.x, min(span.y, t), AIR_STEPS);
            color = color * air.transmittance + air.radiance;
        }
    }
    return vec4<f32>(color, 1.0);
}
