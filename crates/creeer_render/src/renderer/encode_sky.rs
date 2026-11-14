//! The sky's part of the frame's globals, and the sky radiance table the terrain is lit by.

use super::*;

/// Atmosphere part of the globals (all zero when there is no sky).
pub(super) fn sky_globals(sky: Option<&Sky>, camera: &Camera) -> Globals {
    let mut globals = Globals::zeroed();
    let Some(sky) = sky else {
        return globals;
    };
    #[allow(clippy::cast_possible_truncation)] // physical constants fit f32 comfortably
    let f = |v: f64| v as f32;
    let a = &sky.atmosphere;
    let (height, _) = sky.height_and_up(camera.position);
    globals.atmo_rayleigh = [
        f(a.rayleigh_scattering[0]),
        f(a.rayleigh_scattering[1]),
        f(a.rayleigh_scattering[2]),
        f(a.rayleigh_scale_height),
    ];
    globals.atmo_mie = [
        f(a.mie_scattering),
        f(a.mie_extinction),
        f(a.mie_scale_height),
        f(a.mie_g),
    ];
    // The camera height comes from f64: subtracting the radius in f32 would lose metres.
    globals.atmo_geometry = [f(a.planet_radius), f(a.height), f(height), 1.0];
    let [r, g, b] = sky.sun_irradiance.map(f);
    globals.sun_irradiance = [r, g, b, sky.star_brightness];
    let [r, g, b] = a.ground_albedo.map(f);
    globals.ground_albedo = [r, g, b, 0.0];
    if let Some(sea) = &sky.sea {
        // The camera's height over the sea's ellipsoid, stretched to a sphere (f64: `sea.wgsl`).
        let k = sea.axis.normalize();
        let stretch = sea.equatorial_radius / sea.polar_radius - 1.0;
        let q = camera.position - sky.planet_center;
        let stretched = q + k * (stretch * q.dot(k));
        let over = stretched.length() - sea.equatorial_radius;
        globals.sea = [1.0, f(sea.polar_radius), f(sea.equatorial_radius), f(over)];
        globals.sea_axis = [f(k.x), f(k.y), f(k.z), 0.0];
        let c = sky.planet_center - camera.position;
        globals.sea_center = [f(c.x), f(c.y), f(c.z), 0.0];
        let [r, g, b] = sea.color;
        globals.sea_color = [r, g, b, 0.0];
    }
    globals
}

impl Renderer {
    /// The sky radiance table for terrain lighting (`terrain.wgsl`): the cosine-weighted
    /// radiance of the sky at the ground for sun cosines from −0.2 to 1, integrated once
    /// per atmosphere and sun (`atmosphere::local_light`).
    pub(super) fn sky_table(&mut self, sky: Option<&Sky>) -> [[f32; 4]; SKY_TABLE] {
        let Some(sky) = sky else {
            return [[0.0; 4]; SKY_TABLE];
        };
        if let Some((atmosphere, sun, table)) = &self.sky_table
            && *atmosphere == sky.atmosphere
            && sun.map(f64::to_bits) == sky.sun_irradiance.map(f64::to_bits)
        {
            return *table;
        }
        let table = std::array::from_fn(|i| {
            #[allow(clippy::cast_precision_loss)] // 16 entries
            let mu_s = -0.2 + 1.2 * i as f64 / (SKY_TABLE - 1) as f64;
            let to_sun = creeer_math::glam::DVec3::new((1.0 - mu_s * mu_s).sqrt(), mu_s, 0.0);
            let light = crate::atmosphere::local_light(
                &sky.atmosphere,
                0.0,
                creeer_math::glam::DVec3::Y,
                to_sun,
                sky.sun_irradiance,
            );
            #[allow(clippy::cast_possible_truncation)] // light values are small
            let f = |v: f64| v as f32;
            [f(light.sky[0]), f(light.sky[1]), f(light.sky[2]), 0.0]
        });
        self.sky_table = Some((sky.atmosphere, sky.sun_irradiance, table));
        table
    }
}
