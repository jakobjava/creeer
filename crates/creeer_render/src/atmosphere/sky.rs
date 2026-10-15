//! A planet with an atmosphere, and the object lighting it implies.

use super::*;
use crate::scene::Lighting;

/// A planet with an atmosphere, lit by a sun: everything the sky pass needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sky {
    /// Optical properties.
    pub atmosphere: Atmosphere,
    /// Planet centre in simulation space (m).
    pub planet_center: DVec3,
    /// Sun irradiance at the top of the atmosphere (linear RGB, renderer units).
    pub sun_irradiance: [f64; 3],
    /// Brightness of the star field (0 = none).
    pub star_brightness: f32,
}

impl Sky {
    /// Height of `position` above the planet surface (m) and the local up direction.
    pub fn height_and_up(&self, position: DVec3) -> (f64, DVec3) {
        let offset = position - self.planet_center;
        let distance = offset.length();
        let up = if distance > 0.0 {
            offset / distance
        } else {
            DVec3::Y
        };
        (distance - self.atmosphere.planet_radius, up)
    }

    /// Object lighting at `position` for a sun in direction `to_sun`, consistent with the
    /// sky drawn around it.
    pub fn lighting(&self, position: DVec3, to_sun: DVec3, base: &Lighting) -> Lighting {
        let (h, up) = self.height_and_up(position);
        let light = local_light(
            &self.atmosphere,
            h.max(0.0),
            up,
            to_sun,
            self.sun_irradiance,
        );
        #[allow(clippy::cast_possible_truncation)] // light values are small
        let to_f32 = |c: [f64; 3]| c.map(|v| v as f32);
        Lighting {
            sun_direction: to_sun,
            sun_color: to_f32(light.sun),
            sky_ambient: to_f32(light.sky),
            ground_ambient: to_f32(light.ground),
            up,
            ..*base
        }
    }
}
