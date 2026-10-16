//! Shared fixtures for the tests: a KSP-scale star, home planet and moon.

use crate::body::{Body, BodyKind, Spin};
use crate::ephemeris::Ephemeris;
use crate::kepler::Elements;
use creeer_math::glam::DVec3;

pub const SUN_MU: f64 = 1.172_332_8e18;
pub const HOME_MU: f64 = 9.81 * 5e5 * 5e5;
pub const HOME_RADIUS: f64 = 5e5;
pub const MOON_MU: f64 = 6.5e10;
pub const MOON_RADIUS: f64 = 2e5;
pub const HOME_ORBIT: f64 = 1.36e10;
pub const MOON_ORBIT: f64 = 1.2e7;

pub fn body(name: &str, kind: BodyKind, mu: f64, radius: f64) -> Body {
    Body {
        name: name.into(),
        kind,
        mu,
        radius,
        spin: Spin::new(21_600.0, 0.0, 0.0),
        color: [200; 3],
        seed: 1,
        surface: None,
    }
}

/// Sun (index 0), Home (1) on a near-circular orbit, Moon (2) about Home.
pub fn system() -> Ephemeris {
    let (rp, vp) =
        Elements::from_semi_major_axis(SUN_MU, HOME_ORBIT, 0.01, [0.0, 0.0, 0.0, 0.3]).to_state();
    let (rm, vm) =
        Elements::from_semi_major_axis(HOME_MU, MOON_ORBIT, 0.0, [0.05, 0.0, 0.0, 1.0]).to_state();
    let bodies = vec![
        body("Sun", BodyKind::Star, SUN_MU, 2.6e8),
        body("Home", BodyKind::Planet, HOME_MU, HOME_RADIUS),
        body("Moon", BodyKind::Moon, MOON_MU, MOON_RADIUS),
    ];
    Ephemeris::new(
        bodies,
        vec![DVec3::ZERO, rp, rp + rm],
        vec![DVec3::ZERO, vp, vp + vm],
        0.0,
    )
}
