// Sky pass: atmosphere (single scattering + Hillaire's multiple-scattering approximation,
// `atmosphere.wgsl`), the planet surface seen from above, the sun disk and stars. Drawn
// only where no geometry was rendered.

@group(1) @binding(0) var transmittance_lut: texture_2d<f32>;
@group(1) @binding(1) var multiscatter_lut: texture_2d<f32>;
@group(1) @binding(2) var lut_sampler: sampler;

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

const VIEW_STEPS: u32 = 24u;
// Angular radius of the sun as seen from 1 AU (0.27°).
const SUN_COS_RADIUS: f32 = 0.99998889;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> SkyOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    let ndc = uv * 2.0 - 1.0;
    var out: SkyOut;
    // Depth 0 = infinitely far (reversed-Z): only pixels nothing else covered pass.
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.ndc = ndc;
    return out;
}

/// Cheap hash-based star field in world-fixed directions.
fn stars(dir: vec3<f32>) -> vec3<f32> {
    let cells = 220.0;
    let cell = floor(dir * cells);
    let hash = fract(sin(dot(cell, vec3<f32>(12.9898, 78.233, 37.719))) * 43758.5453);
    if hash < 0.985 {
        return vec3<f32>(0.0);
    }
    // Star position within its cell, and a soft point of light around it.
    let jitter = fract(vec3<f32>(hash * 7.13, hash * 3.71, hash * 5.37));
    let center = normalize((cell + 0.25 + 0.5 * jitter) / cells);
    let d = 1.0 - dot(dir, center);
    let brightness = pow((hash - 0.985) / 0.015, 6.0) * 4.0 + 0.3;
    let tint = mix(vec3<f32>(0.75, 0.82, 1.0), vec3<f32>(1.0, 0.85, 0.7), fract(hash * 91.7));
    return tint * brightness * exp(-d * 4.0e6);
}

@fragment
fn fs_main(in: SkyOut) -> @location(0) vec4<f32> {
    // Camera-relative point on the near plane → view direction.
    let near = globals.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let dir = normalize(near.xyz / near.w);
    let up = globals.up.xyz;
    let to_sun = globals.sun_direction.xyz;
    let sun_toa = globals.sun_irradiance.rgb;

    let h = globals.atmo_geometry.z;
    let mu = dot(dir, up);
    let mu_s = dot(up, to_sun);
    let nu = dot(dir, to_sun);
    let r = planet_radius() + h;

    let ground = ground_distance(h, mu);
    let span = atmosphere_span(h, mu);
    var radiance = vec3<f32>(0.0);
    var view_transmittance = vec3<f32>(1.0);

    if span.y > 0.0 {
        var end = span.y;
        if ground >= 0.0 {
            end = min(end, ground);
        }
        let air = inscatter(h, mu, mu_s, nu, span.x, end, VIEW_STEPS);
        radiance = air.radiance;
        view_transmittance = air.transmittance;
    }

    if ground >= 0.0 {
        // Lambertian ground lit by the attenuated sun and the sky.
        let h_g = height_at(h, mu, ground);
        let r_g = planet_radius() + h_g;
        let mu_s_g = (r * mu_s + ground * nu) / r_g;
        let sun_g = sun_toa * transmittance_to_space(max(h_g, 0.0), mu_s_g) * max(mu_s_g, 0.0);
        let sky_g = globals.sky_ambient.rgb * PI;
        let ground_radiance = globals.ground_albedo.rgb / PI * (sun_g + sky_g);
        radiance += ground_radiance * view_transmittance;
    } else {
        // Open sky: sun disk and stars, dimmed by the air in front of them.
        if nu > SUN_COS_RADIUS {
            let solid_angle = 2.0 * PI * (1.0 - SUN_COS_RADIUS);
            // True disk radiance (~5·10⁴) is near the f16 limit; scaled down, it still
            // saturates to white after tonemapping.
            radiance += sun_toa / solid_angle * view_transmittance * 0.02;
        }
        // Stars disappear against a bright sky (as they do for eyes and cameras).
        let sky_luminance = dot(radiance, vec3<f32>(0.2126, 0.7152, 0.0722));
        let visibility = clamp(1.0 - sky_luminance * 40.0, 0.0, 1.0);
        radiance += stars(dir) * globals.sun_irradiance.w * view_transmittance * visibility;
    }
    return vec4<f32>(radiance, 1.0);
}
