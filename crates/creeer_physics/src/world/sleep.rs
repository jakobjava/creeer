//! Sleeping: bodies that have come to rest stop being stepped until something disturbs
//! them. A sleeping body still collides (others rest on it); it wakes when an awake body
//! touches it, when its loads change, or when its velocity is set.

use super::{Body, World};
use creeer_math::glam::DVec3;

/// Below these speeds (m/s, rad/s) at every node a body counts as still.
const STILL_SPEED: f64 = 1.0e-3;
const STILL_SPIN: f64 = 1.0e-3;
/// Still for this many steps (1 s at 60 Hz): asleep.
const STEPS_TO_SLEEP: u32 = 60;

/// Sleep bookkeeping of one body.
#[derive(Clone, Debug, Default)]
pub struct Sleep {
    /// Whether the body is asleep (not stepped).
    pub asleep: bool,
    still: u32,
    /// Loads when it fell asleep: any change wakes it.
    load: Vec<DVec3>,
}

impl Body {
    /// Wakes the body (it is stepped again from the next step).
    pub fn wake(&mut self) {
        if self.sleep.asleep {
            self.sleep.asleep = false;
            self.sleep.still = 0;
            self.solver.reset_history(&self.structure);
        }
    }

    fn is_still(&self) -> bool {
        let s = &self.structure;
        s.velocity.iter().all(|v| v.length() < STILL_SPEED)
            && s.angular_velocity.iter().all(|w| w.length() < STILL_SPIN)
    }
}

impl World {
    /// Wakes sleeping bodies whose loads or velocities were changed from outside.
    pub(super) fn wake_disturbed(&mut self) {
        for body in self.bodies.iter_mut().filter(|b| b.sleep.asleep) {
            let s = &body.structure;
            let moved = s
                .velocity
                .iter()
                .chain(&s.angular_velocity)
                .any(|v| *v != DVec3::ZERO);
            if moved || s.load != body.sleep.load {
                body.wake();
            }
        }
    }

    /// Wakes every sleeping body with a collision shape within `radius` of `center`: call it
    /// after the ground changed there (a terrain edit), or a body resting on removed ground
    /// would float.
    pub fn wake_near(&mut self, center: DVec3, radius: f64) {
        for body in self.bodies.iter_mut().filter(|b| b.sleep.asleep) {
            let s = &body.structure;
            let near = body.shapes.iter().any(|&(node, shape)| {
                (s.position[node] - center).length() < radius + shape.bounding_radius()
            });
            if near {
                body.wake();
            }
        }
    }

    /// After a step: counts still steps, and puts whole groups of touching bodies to sleep
    /// once all their bodies have been still long enough (one body falling asleep alone
    /// would be woken again at once by its still-awake neighbour).
    pub(super) fn update_sleep(&mut self, groups: &[Vec<usize>]) {
        for body in self.bodies.iter_mut().filter(|b| !b.sleep.asleep) {
            if body.is_still() {
                body.sleep.still += 1;
            } else {
                body.sleep.still = 0;
            }
        }
        for group in groups {
            if group
                .iter()
                .all(|&b| self.bodies[b].sleep.still >= STEPS_TO_SLEEP)
            {
                for &b in group {
                    let body = &mut self.bodies[b];
                    body.sleep.asleep = true;
                    body.sleep.load.clone_from(&body.structure.load);
                    let s = &mut body.structure;
                    s.velocity.fill(DVec3::ZERO);
                    s.angular_velocity.fill(DVec3::ZERO);
                }
            }
        }
    }

    /// Number of bodies asleep.
    pub fn sleeping(&self) -> usize {
        self.bodies.iter().filter(|b| b.sleep.asleep).count()
    }
}
