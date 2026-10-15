// Sky pass: atmosphere (single scattering + Hillaire's multiple-scattering approximation),
// the planet surface seen from above, the sun disk and stars. Mirrors `atmosphere.rs`: same
// equations, same cancellation-free height formulas, same table parameterisation. Drawn
// only where no geometry was rendered.

@group(1) @binding(0) var transmittance_lut: texture_2d<f32>;
@group(1) @binding(1) var multiscatter_lut: texture_2d<f32>;
@group(1) @binding(2) var lut_sampler: sampler;

struct SkyOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

const PI: f32 = 3.14159265;
const VIEW_STEPS: u32 = 24u;
// Must match TRANSMITTANCE_LUT and MULTISCATTER_LUT in atmosphere.rs.
const TRANSMITTANCE_SIZE: vec2<f32> = vec2<f32>(256.0, 64.0);
const MULTISCATTER_SIZE: f32 = 32.0;
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

fn planet_radius() -> f32 { return globals.atmo_geometry.x; }
fn atmosphere_height() -> f32 { return globals.atmo_geometry.y; }

/// Height above ground after travelling t along a ray from height h with vertical cosine mu.
fn height_at(h: f32, mu: f32, t: f32) -> f32 {
    let big_r = planet_radius();
    let r = big_r + h;
    let numerator = h * (2.0 * big_r + h) + t * t + 2.0 * r * mu * t;
    let r_t = sqrt(max(r * r + t * t + 2.0 * r * mu * t, 0.0));
    return numerator / (r_t + big_r);
}

/// Distance to the ground, or -1 if the ray misses the planet.
fn ground_distance(h: f32, mu: f32) -> f32 {
    let r = planet_radius() + h;
    let discriminant = r * r * mu * mu - h * (2.0 * planet_radius() + h);
    if mu < 0.0 && discriminant >= 0.0 {
        return max(-r * mu - sqrt(discriminant), 0.0);
    }
    return -1.0;
}

/// (enter, exit) distances through the atmosphere shell; exit < 0 if missed.
fn atmosphere_span(h: f32, mu: f32) -> vec2<f32> {
    let r = planet_radius() + h;
    let top = planet_radius() + atmosphere_height();
    let discriminant = r * r * mu * mu + (atmosphere_height() - h) * (top + r);
    if discriminant < 0.0 {
        return vec2<f32>(0.0, -1.0);
    }
    let root = sqrt(discriminant);
    return vec2<f32>(max(-r * mu - root, 0.0), -r * mu + root);
}

fn density_rayleigh(h: f32) -> f32 { return exp(-max(h, 0.0) / globals.atmo_rayleigh.w); }
fn density_mie(h: f32) -> f32 { return exp(-max(h, 0.0) / globals.atmo_mie.z); }

fn extinction(h: f32) -> vec3<f32> {
    return globals.atmo_rayleigh.rgb * density_rayleigh(h) + vec3<f32>(density_mie(h) * globals.atmo_mie.y);
}

fn scattering(h: f32) -> vec3<f32> {
    return globals.atmo_rayleigh.rgb * density_rayleigh(h) + vec3<f32>(density_mie(h) * globals.atmo_mie.x);
}

/// Table coordinate for a parameter in [0, 1] over `size` texels (first/last texel centres).
fn texel_coordinate(x: vec2<f32>, size: vec2<f32>) -> vec2<f32> {
    return (clamp(x, vec2<f32>(0.0), vec2<f32>(1.0)) * (size - 1.0) + 0.5) / size;
}

/// Transmittance from height h along vertical cosine mu out of the atmosphere (table lookup
/// with Bruneton's horizon-aware mapping; zero if the planet blocks the ray).
fn transmittance_to_space(h_in: f32, mu: f32) -> vec3<f32> {
    if ground_distance(h_in, mu) >= 0.0 {
        return vec3<f32>(0.0);
    }
    let big_r = planet_radius();
    let height = atmosphere_height();
    let h = clamp(h_in, 0.0, height);
    let extent = sqrt(height * (2.0 * big_r + height));
    let rho = sqrt(h * (2.0 * big_r + h));
    let r = big_r + h;
    let top_minus_r = height - h;
    let discriminant = max(r * r * mu * mu + top_minus_r * (big_r + height + r), 0.0);
    let d = max(-r * mu + sqrt(discriminant), 0.0);
    let uv = vec2<f32>((d - top_minus_r) / (rho + extent - top_minus_r), rho / extent);
    return textureSampleLevel(transmittance_lut, lut_sampler, texel_coordinate(uv, TRANSMITTANCE_SIZE), 0.0).rgb;
}

/// Multiple-scattering source Ψ (per unit sun irradiance) at height h for sun cosine mu_s.
fn multiscatter(h: f32, mu_s: f32) -> vec3<f32> {
    let uv = vec2<f32>(mu_s * 0.5 + 0.5, h / atmosphere_height());
    return textureSampleLevel(multiscatter_lut, lut_sampler, texel_coordinate(uv, vec2<f32>(MULTISCATTER_SIZE)), 0.0).rgb;
}

fn rayleigh_phase(nu: f32) -> f32 {
    return 3.0 / (16.0 * PI) * (1.0 + nu * nu);
}

fn mie_phase(g: f32, nu: f32) -> f32 {
    let g2 = g * g;
    return 3.0 / (8.0 * PI) * (1.0 - g2) * (1.0 + nu * nu)
        / ((2.0 + g2) * pow(1.0 + g2 - 2.0 * g * nu, 1.5));
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
        let dt = (end - span.x) / f32(VIEW_STEPS);
        let phase_r = rayleigh_phase(nu);
        let phase_m = mie_phase(globals.atmo_mie.w, nu);
        var depth = vec3<f32>(0.0);
        for (var i = 0u; i < VIEW_STEPS; i++) {
            let t = span.x + (f32(i) + 0.5) * dt;
            let h_t = height_at(h, mu, t);
            let half_depth = extinction(h_t) * dt * 0.5;
            depth += half_depth;
            let r_t = r + (h_t - h);
            let mu_s_t = (r * mu_s + t * nu) / r_t;
            let sun_t = transmittance_to_space(h_t, mu_s_t);
            let single = globals.atmo_rayleigh.rgb * density_rayleigh(h_t) * phase_r
                + vec3<f32>(globals.atmo_mie.x * density_mie(h_t) * phase_m);
            let source = single * sun_t + scattering(h_t) * multiscatter(h_t, mu_s_t);
            radiance += source * exp(-depth) * sun_toa * dt;
            depth += half_depth;
        }
        view_transmittance = exp(-depth);
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
