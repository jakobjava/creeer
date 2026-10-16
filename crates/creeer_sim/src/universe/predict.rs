//! Predicted trajectories for the map view, in a chosen plotting frame (ADR-0007).
//!
//! Nothing moves on a fixed conic here, so the future is computed, not drawn from orbital
//! elements: the ship is propagated on rails through the forecast of the bodies. How the
//! path looks depends on the frame it is drawn in (as in the Principia mod): centred on a
//! body (non-rotating), the path around that body; turning with a pair of bodies, the paths
//! near their Lagrange points, which stand still there.

use super::Universe;
use super::terrains::BodyRef;
use creeer_math::glam::DVec3;
use creeer_orbit::analysis::{self, lagrange_points};
use creeer_orbit::rails::{Group, Particle};

/// The frame a trajectory is drawn in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlotFrame {
    /// Centred on a body, inertial axes.
    Body(BodyRef),
    /// Origin at major `.0`, x towards major `.1`, z along their orbital angular momentum.
    Corotating(usize, usize),
}

/// A predicted path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Prediction {
    /// Sample times (s).
    pub times: Vec<f64>,
    /// Positions in the plotting frame at those times (m).
    pub points: Vec<DVec3>,
    /// Whether the path ends on a body's surface.
    pub impact: Option<BodyRef>,
}

impl Universe {
    /// Position of an inertial point at time `t` in a plotting frame.
    pub fn to_plot(&self, frame: PlotFrame, x: DVec3, t: f64) -> DVec3 {
        match frame {
            PlotFrame::Body(body) => {
                let at = match body {
                    BodyRef::Major(i) => self.bodies.ephemeris.position(i, t),
                    // Minors are known now only; near one, its motion over a prediction is
                    // what the map can show anyway.
                    BodyRef::Minor(_) => self.bodies.state(body, self.time()).0,
                };
                x - at
            }
            PlotFrame::Corotating(p, s) => {
                let eph = &self.bodies.ephemeris;
                analysis::to_corotating(x, eph.state(p, t), eph.state(s, t))
            }
        }
    }

    /// The path of something at inertial state `(x, v)` now, `duration` seconds ahead in
    /// `samples` points, drawn in `frame`; it ends where it would hit a body.
    pub fn predict(
        &mut self,
        (x, v): (DVec3, DVec3),
        duration: f64,
        samples: u32,
        frame: PlotFrame,
    ) -> Prediction {
        let t0 = self.time();
        self.bodies.ephemeris.extend_to(t0 + duration);
        let mut group = Group::new(t0, &[(x, v)], vec![Particle { mass: 1.0, mu: 0.0 }]);
        let mut prediction = Prediction::default();
        prediction.times.push(t0);
        prediction.points.push(self.to_plot(frame, x, t0));
        for k in 1..=samples {
            let t = t0 + duration * f64::from(k) / f64::from(samples);
            let hit = group.propagate(&self.bodies.ephemeris, t, Some(0.0));
            let now = group.time();
            prediction.times.push(now);
            prediction
                .points
                .push(self.to_plot(frame, group.state(0).0, now));
            if let Some(hit) = hit {
                prediction.impact = Some(BodyRef::Major(hit.body));
                break;
            }
        }
        prediction
    }

    /// The five Lagrange points of majors `p` (primary) and `s` (secondary) now, inertial.
    pub fn lagrange_points(&self, p: usize, s: usize) -> [DVec3; 5] {
        let eph = &self.bodies.ephemeris;
        let t = self.time();
        let ((xp, vp), (xs, vs)) = (eph.state(p, t), eph.state(s, t));
        lagrange_points((xp, vp, eph.bodies()[p].mu), (xs, vs, eph.bodies()[s].mu))
    }

    /// The major body an inertial point orbits now (smallest sphere of influence around it).
    pub fn primary_of(&self, x: DVec3) -> usize {
        let eph = &self.bodies.ephemeris;
        let t = self.time();
        let mu: Vec<f64> = eph.bodies().iter().map(|b| b.mu).collect();
        let xs: Vec<DVec3> = (0..eph.len()).map(|i| eph.position(i, t)).collect();
        let parents = analysis::primaries(&mu, &xs);
        analysis::primary_of(&mu, &xs, &parents, x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universe::tests_support::universe;
    use creeer_orbit::kepler::Elements;

    #[test]
    fn a_low_orbit_predicts_a_closed_ellipse_around_home() {
        let mut u = universe();
        let home = u.home;
        let t = u.time();
        let (hx, hv) = u.bodies.ephemeris.state(home, t);
        let mu = u.bodies.ephemeris.bodies()[home].mu;
        let el = Elements::from_semi_major_axis(mu, 7.0e5, 0.05, [0.3, 0.2, 0.1, 0.0]);
        let (r, w) = el.to_state();
        let p = u.predict(
            (hx + r, hv + w),
            el.period(),
            400,
            PlotFrame::Body(BodyRef::Major(home)),
        );
        assert_eq!(p.impact, None);
        assert_eq!(u.primary_of(hx + r), home);
        // After one period it is back where it started (the moon and the sun perturb it a
        // little: metres on a 700 km orbit).
        let gap = (p.points[p.points.len() - 1] - p.points[0]).length();
        assert!(gap < 200.0, "{gap} m");
        // And every point lies between periapsis and apoapsis.
        for q in &p.points {
            let d = q.length();
            assert!(
                d > el.periapsis() * 0.999 && d < el.apoapsis() * 1.001,
                "{d}"
            );
        }
    }

    #[test]
    fn a_fall_ends_on_the_surface() {
        let mut u = universe();
        let home = u.home;
        let (hx, hv) = u.bodies.ephemeris.state(home, u.time());
        let p = u.predict(
            (hx + DVec3::Y * 6e5, hv),
            3600.0,
            200,
            PlotFrame::Body(BodyRef::Major(home)),
        );
        assert_eq!(p.impact, Some(BodyRef::Major(home)));
        let last = p.points[p.points.len() - 1].length();
        assert!((last - 5e5).abs() < 1.0, "{last}");
    }

    #[test]
    fn lagrange_points_stand_still_in_the_corotating_frame() {
        let u = universe();
        let home = u.home;
        let moon = (0..u.parents.len())
            .find(|&i| u.parents[i] == Some(home))
            .expect("home has a moon");
        let l = u.lagrange_points(home, moon);
        let (t, frame) = (u.time(), PlotFrame::Corotating(home, moon));
        let d =
            (u.bodies.ephemeris.position(moon, t) - u.bodies.ephemeris.position(home, t)).length();
        // L4 is 60° ahead of the moon at the moon's distance, in the orbital plane.
        let l4 = u.to_plot(frame, l[3], t);
        assert!((l4.length() / d - 1.0).abs() < 1e-9);
        // Ahead of the moon is −z in the Y-up co-rotating axes; the orbit plane is y = 0.
        assert!((l4.x / d - 0.5).abs() < 1e-9 && l4.z < 0.0 && l4.y.abs() < 1e-3);
        // L1 lies between home and the moon on the x axis.
        let l1 = u.to_plot(frame, l[0], t);
        assert!(l1.x > 0.5 * d && l1.x < d && l1.y.abs() < 1e-3 && l1.z.abs() < 1e-3);
    }
}
