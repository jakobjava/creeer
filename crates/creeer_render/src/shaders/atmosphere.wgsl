// Atmosphere functions shared by the sky and the terrain (aerial perspective). Mirrors
// `atmosphere.rs`: same equations, same cancellation-free height formulas, same table
// parameterisation. The including shader declares `transmittance_lut`, `multiscatter_lut`
// and `lut_sampler` in its own bind group.

// Must match TRANSMITTANCE_LUT and MULTISCATTER_LUT in atmosphere.rs.
const TRANSMITTANCE_SIZE: vec2<f32> = vec2<f32>(256.0, 64.0);
const MULTISCATTER_SIZE: f32 = 32.0;

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


/// Light scattered towards the viewer along a view ray, and the transmittance of the ray.
struct Inscatter {
    radiance: vec3<f32>,
    transmittance: vec3<f32>,
};

/// In-scattering along the view ray from height h (vertical cosine mu, sun cosine mu_s,
/// view–sun cosine nu) between distances t0 and t1, in `steps` midpoint steps: single
/// scattering of the sun plus Hillaire's multiple-scattering term, per unit of
/// `globals.sun_irradiance`.
fn inscatter(h: f32, mu: f32, mu_s: f32, nu: f32, t0: f32, t1: f32, steps: u32) -> Inscatter {
    var out: Inscatter;
    out.radiance = vec3<f32>(0.0);
    out.transmittance = vec3<f32>(1.0);
    if t1 <= t0 {
        return out;
    }
    let r = planet_radius() + h;
    let sun_toa = globals.sun_irradiance.rgb;
    let dt = (t1 - t0) / f32(steps);
    let phase_r = rayleigh_phase(nu);
    let phase_m = mie_phase(globals.atmo_mie.w, nu);
    var depth = vec3<f32>(0.0);
    for (var i = 0u; i < steps; i++) {
        let t = t0 + (f32(i) + 0.5) * dt;
        let h_t = height_at(h, mu, t);
        let half_depth = extinction(h_t) * dt * 0.5;
        depth += half_depth;
        let r_t = r + (h_t - h);
        let mu_s_t = (r * mu_s + t * nu) / r_t;
        let sun_t = transmittance_to_space(h_t, mu_s_t);
        let single = globals.atmo_rayleigh.rgb * density_rayleigh(h_t) * phase_r
            + vec3<f32>(globals.atmo_mie.x * density_mie(h_t) * phase_m);
        let source = single * sun_t + scattering(h_t) * multiscatter(h_t, mu_s_t);
        out.radiance += source * exp(-depth) * sun_toa * dt;
        depth += half_depth;
    }
    out.transmittance = exp(-depth);
    return out;
}
