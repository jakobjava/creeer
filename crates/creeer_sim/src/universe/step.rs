//! Advancing the universe by one tick: orbits, rails, the bubble, then frame and range.

use super::bubble::{Bubble, Frame};
use super::terrains::BodyRef;
use super::vessels::{self, Where};
use super::{
    Event, Focus, LOOKAHEAD, MAX_PHYSICS_WARP, MAX_WARP, RANGE_IN, RANGE_OUT, REBASE_DISTANCE,
    REBASE_SPEED, Universe, ground, surface_zone,
};
use creeer_math::glam::DVec3;
use rayon::prelude::*;

impl Universe {
    /// Advances by one tick of real time (the physics step), times the warp.
    pub fn step(&mut self) {
        let tick = self.bubble.world.dt();
        let physics = self.warp <= MAX_PHYSICS_WARP;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 1..=4
        let substeps = if physics {
            (self.warp.round() as u32).max(1)
        } else {
            0
        };
        let t0 = self.time;
        let span = if physics {
            tick * f64::from(substeps)
        } else {
            tick * self.warp
        };
        let t1 = t0 + span;
        self.bodies.ephemeris.extend_to(t1 + LOOKAHEAD);

        let anchored_minor = match self.bubble.frame {
            Frame::Surface(BodyRef::Minor(i)) => Some(i),
            _ => None,
        };
        for (minor, body) in self.bodies.propagate_minors(t1, anchored_minor) {
            self.events.push(Event::MinorImpact { minor, body });
        }
        self.propagate_rails(t1);

        if physics && !self.bubble.vessels.is_empty() {
            let h = self.bubble.world.dt();
            for k in 0..substeps {
                self.bubble.step(&mut self.bodies, t0 + f64::from(k) * h);
            }
        } else {
            // Nothing to simulate: only the frame moves on.
            match &mut self.bubble.frame {
                Frame::Orbit(origin) => {
                    origin.propagate(&self.bodies.ephemeris, t1, None);
                }
                Frame::Surface(BodyRef::Minor(i)) => {
                    let i = *i;
                    self.bodies.minors[i]
                        .group
                        .propagate(&self.bodies.ephemeris, t1, None);
                }
                Frame::Surface(BodyRef::Major(_)) => {}
            }
        }
        self.time = t1;
        self.bubble
            .flush_reaction(&mut self.bodies, t1, span.max(1e-3), false);
        self.check_majors();
        self.manage_frame();
        if physics {
            self.manage_range();
        }
        self.bodies.ephemeris.forget_before(t1 - 60.0);
    }

    /// Sets the time warp. Beyond the physics warp every vessel is packed; that needs all of
    /// them at rest or off the ground.
    ///
    /// # Errors
    /// If something still moves on the ground.
    pub fn set_warp(&mut self, warp: f64) -> Result<(), String> {
        let warp = warp.clamp(1.0, MAX_WARP);
        if warp > MAX_PHYSICS_WARP {
            if self.bubble.anchor().is_some() {
                let moving = self
                    .bubble
                    .world
                    .bodies
                    .iter()
                    .any(|b| !b.sleep.asleep && self.on_ground(b));
                if moving {
                    return Err("objects are still moving on the ground".into());
                }
            }
            let pose = self.bubble_pose();
            while !self.bubble.vessels.is_empty() {
                let id = self.bubble.vessels[0];
                let place = vessels::pack(&mut self.bubble, &pose, 0, self.time);
                self.place(id, place);
            }
        }
        self.warp = warp;
        Ok(())
    }

    /// Whether a body touches the ground (within 5 cm of it).
    fn on_ground(&self, body: &creeer_physics::world::Body) -> bool {
        let Some(ground) = &self.bubble.world.ground else {
            return false;
        };
        body.structure
            .position
            .iter()
            .any(|&p| ground.density_gradient(p).0 < 2.0)
    }

    fn place(&mut self, id: u64, place: Where) {
        if let Some(v) = self.vessels.iter_mut().find(|v| v.id == id) {
            v.place = place;
        }
    }

    /// Propagates every vessel on rails to `t`; one that hits a body comes to rest there.
    fn propagate_rails(&mut self, t: f64) {
        let eph = &self.bodies.ephemeris;
        let hits: Vec<(usize, usize, f64)> = self
            .vessels
            .par_iter_mut()
            .enumerate()
            .filter_map(|(i, v)| match &mut v.place {
                Where::Rails { group, .. } => group
                    .propagate(eph, t, Some(0.0))
                    .map(|hit| (i, hit.body, hit.t)),
                _ => None,
            })
            .collect();
        for (i, body, when) in hits {
            let on = BodyRef::Major(body);
            let id = self.vessels[i].id;
            let place = std::mem::replace(&mut self.vessels[i].place, Where::Bubble);
            // Where it hit, in the body's frame at that moment.
            let at = self.bodies.pose(on, when);
            let mut stopped = vessels::unpack(place, &self.bodies, &at, when);
            stopped.structure.velocity.fill(DVec3::ZERO);
            stopped.structure.angular_velocity.fill(DVec3::ZERO);
            self.vessels[i].place = Where::Landed { on, body: stopped };
            self.events.push(Event::VesselImpact {
                vessel: id,
                body: on,
            });
            self.warp = self.warp.min(1.0);
        }
    }

    /// Reports major bodies that touch, and stops time warp for it.
    fn check_majors(&mut self) {
        let eph = &self.bodies.ephemeris;
        let t = self.time;
        let n = eph.len();
        let x: Vec<DVec3> = (0..n).map(|i| eph.position(i, t)).collect();
        for i in 0..n {
            for j in i + 1..n {
                let (ri, rj) = (eph.bodies()[i].radius, eph.bodies()[j].radius);
                if (x[i] - x[j]).length() < ri + rj {
                    self.events.push(Event::MajorContact { bodies: (i, j) });
                    self.warp = self.warp.min(1.0);
                }
            }
        }
    }

    /// Switches between the surface frame of the body the focus is near and a free-falling
    /// frame, and keeps a free-falling origin near the focus.
    fn manage_frame(&mut self) {
        let t = self.time;
        let (fx, fv) = self.focus_state();
        let (local, local_v) = self.focus_local();
        let next = match &self.bubble.frame {
            Frame::Surface(body) => {
                let b = self.bodies.body(*body);
                let height = local.length() - b.radius;
                let airborne = self.bubble.world.bodies.iter().all(|b| !self.on_ground(b));
                (height > 1.2 * surface_zone(b.radius) && airborne)
                    .then(|| Frame::Orbit(Bubble::origin(t, fx, fv)))
            }
            Frame::Orbit(_) => {
                let near = self
                    .bodies
                    .all()
                    .filter(|(_, b)| b.kind.has_surface())
                    .map(|(r, b)| {
                        let d = (self.bodies.state(r, t).0 - fx).length();
                        (r, d - b.radius - surface_zone(b.radius))
                    })
                    .filter(|(_, inside)| *inside < 0.0)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                match near {
                    Some((body, _)) => Some(Frame::Surface(body)),
                    None if local.length() > REBASE_DISTANCE || local_v.length() > REBASE_SPEED => {
                        Some(Frame::Orbit(Bubble::origin(t, fx, fv)))
                    }
                    None => None,
                }
            }
        };
        let Some(frame) = next else { return };
        let tick = self.bubble.world.dt();
        self.bubble.flush_reaction(&mut self.bodies, t, tick, true);
        let ground = match &frame {
            Frame::Surface(body) => ground(&self.terrains, &self.bodies, *body),
            Frame::Orbit(_) => None,
        };
        self.bubble.change_frame(&self.bodies, t, frame, ground);
        if let (Focus::Point(p), Some(change)) = (self.focus, self.bubble.last_change) {
            self.focus = Focus::Point(change.point(p));
        }
        self.events.push(Event::FrameChanged);
    }

    /// Packs vessels that are too far from the focus, and brings back those near it.
    fn manage_range(&mut self) {
        let t = self.time;
        let pose = self.bubble_pose();
        let (focus, _) = self.focus_local();
        let mut k = 0;
        while k < self.bubble.vessels.len() {
            let (x, _, _) = vessels::centre_of_mass(&self.bubble.world.bodies[k]);
            if (x - focus).length() > RANGE_OUT {
                let id = self.bubble.vessels[k];
                let place = vessels::pack(&mut self.bubble, &pose, k, t);
                self.place(id, place);
            } else {
                k += 1;
            }
        }
        let (focus_inertial, _) = self.focus_state();
        for i in 0..self.vessels.len() {
            let near = vessels::stored_position(&self.vessels[i].place, &self.bodies, t)
                .is_some_and(|p| (p - focus_inertial).length() < RANGE_IN);
            if near {
                let place = std::mem::replace(&mut self.vessels[i].place, Where::Bubble);
                let body = vessels::unpack(place, &self.bodies, &pose, t);
                let field = self.bubble.field(&self.bodies, t);
                self.bubble.world.set_planet(Some(field));
                self.bubble.world.add(body);
                self.bubble.vessels.push(self.vessels[i].id);
            }
        }
    }
}
