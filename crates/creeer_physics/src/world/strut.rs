//! Struts (P1, ADR-0023): one-sided, capped spring-dampers between a point of one body and a
//! point of another (or of the fixed world), along a direction fixed for the step: a
//! character's legs on what they stand on.
//!
//! A strut is part of its bodies' step, solved with them in their group's minimisation beside
//! the joints (`joint/strut.rs`), not a load put on them before it. A load from a stiff spring
//! is stable only on a body heavier than k·h² (8 kg for a leg, 30 kN/m at 60 Hz): a character
//! standing on a ragdoll's hand (0.6 kg) put 3 kN on it as a load, and flung it at 64 rad/s.
//! Solved with both bodies, the hand is pressed into the ground under it, as a hand under a
//! boot is.
//!
//! Its force along the direction (B to A) is k·(rest − d) + c·(rate of compression), the
//! length d measured between its two points, at least nought (it pushes, never pulls) and at
//! most `most`. In the step it is the force of a spring of k + c/β towards a rest shifted by
//! the damping (both act on the same length, and the step's velocity is (d − d̂)/β), clamped:
//! convex, so the minimisation's guarantees hold.
//!
//! A strut may also carry a walk ([`Walk`], G2.10): a damper across its direction, between A
//! and B's point, towards a speed asked for and within a least and a most force each way (a
//! character's walk and its soles' grip on the body it stands on). Its drive's work is booked
//! as a grip's is (`joint/strut.rs`).
//!
//! Struts are set for one step by whoever owns them, and taken off by them after it; they are
//! not saved.

use super::World;
use crate::structure::NodeId;
use creeer_math::glam::DVec3;

/// A strut.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strut {
    /// Side A: body and node, and the point in the node's frame.
    pub a: (usize, NodeId),
    pub at_a: DVec3,
    /// Side B: body and node, or the world; the point in the node's frame (the world's).
    pub b: Option<(usize, NodeId)>,
    pub at_b: DVec3,
    /// From B to A (world; unit): the direction its length is measured and it pushes along.
    pub direction: DVec3,
    /// Its length unloaded (m), stiffness (N/m), damping (N·s/m) and most force (N).
    pub rest: f64,
    pub stiffness: f64,
    pub damping: f64,
    pub most: f64,
    /// The speed it lengthens at (m/s) at which it pushes no more: lengthening at v it pushes
    /// at most most·(1 − v/fastest), as legs do (Hill's force–velocity relation, linear for
    /// the whole limb in jumping: Samozino et al. 2008, G2.10). `f64::INFINITY`: no such limit.
    pub fastest: f64,
    /// Along the ground: a damper on A's speed over B's point (none: it only pushes).
    pub walk: Option<Walk>,
}

/// A strut's damper along the ground (G2.10): a character's walk and its soles' grip on the
/// body it stands on, solved in the step with them. Along each of its two axes, its force on A
/// is c·(the speed asked for − A's speed over B's point at the step's end), within `most`
/// (the least and the most, N): a Huber spring of c/β in the step, convex.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walk {
    /// Its two axes (world; unit, across the strut's direction).
    pub axes: [DVec3; 2],
    /// A's speed over B's point asked for along each (m/s).
    pub speed: [f64; 2],
    /// Its damping (N·s/m), and its least and most force along each axis (N; the least at
    /// most nought, the most at least).
    pub damping: f64,
    pub most: [[f64; 2]; 2],
}

impl Walk {
    /// Its force on A along each axis at A's speed `over` B's point there (m/s).
    pub fn force(&self, over: [f64; 2]) -> [f64; 2] {
        [0, 1].map(|k| {
            let [least, most] = self.most[k];
            (self.damping * (self.speed[k] - over[k])).clamp(least, most)
        })
    }
}

impl Strut {
    /// Its length along its direction, as `world`'s bodies stand (m).
    pub fn length(&self, world: &World) -> f64 {
        let point = |side: Option<(usize, NodeId)>, at: DVec3| match side {
            Some((b, n)) => {
                let s = &world.bodies[b].structure;
                s.position[n] + s.orientation[n] * at
            }
            None => at,
        };
        (point(Some(self.a), self.at_a) - point(self.b, self.at_b)).dot(self.direction)
    }

    /// Its force at length `d` closing at `closing` (m/s: the rate its length shrinks), as
    /// its law gives it (N).
    pub fn force(&self, d: f64, closing: f64) -> f64 {
        let push =
            (self.stiffness * (self.rest - d) + self.damping * closing).clamp(0.0, self.most);
        if closing < 0.0 {
            push.min((self.most * (1.0 + closing / self.fastest)).max(0.0))
        } else {
            push
        }
    }
}

impl World {
    /// The struts for the next step (`strut.rs`): kept until their owner takes them off.
    pub fn struts(&self) -> &[Strut] {
        &self.struts
    }

    /// Sets a strut for the next step; returns its index.
    pub fn add_strut(&mut self, strut: Strut) -> usize {
        self.struts.push(strut);
        self.struts.len() - 1
    }

    /// Strut `k` (`add_strut`'s index), to change before the step.
    pub fn strut_mut(&mut self, k: usize) -> &mut Strut {
        &mut self.struts[k]
    }

    /// What the struts push each body with as the bodies stand and move now (after a step:
    /// at its end, as the step's own force), by body and node: A along its direction, B
    /// against it (N). (A body's velocity change in the step less its gravity and loads is
    /// then what struck it.)
    pub fn strut_pushes(&self) -> Vec<(usize, NodeId, DVec3)> {
        let velocity = |side: Option<(usize, NodeId)>, at: DVec3| match side {
            Some((b, n)) => {
                let s = &self.bodies[b].structure;
                s.velocity[n] + s.angular_velocity[n].cross(s.orientation[n] * at)
            }
            None => DVec3::ZERO,
        };
        let mut out = Vec::new();
        for s in &self.struts {
            let over = velocity(Some(s.a), s.at_a) - velocity(s.b, s.at_b);
            let apart = over.dot(s.direction);
            let mut push = s.direction * s.force(s.length(self), -apart);
            if let Some(walk) = &s.walk {
                let f = walk.force(walk.axes.map(|axis| over.dot(axis)));
                push += walk.axes[0] * f[0] + walk.axes[1] * f[1];
            }
            out.push((s.a.0, s.a.1, push));
            if let Some((b, n)) = s.b {
                out.push((b, n, -push));
            }
        }
        out
    }

    /// Takes every strut off.
    pub fn clear_struts(&mut self) {
        self.struts.clear();
    }

    /// Bodies a strut joins to an awake one wake with it (they are stepped together).
    pub(super) fn wake_strutted(&mut self) -> bool {
        let mut woke = false;
        for k in 0..self.struts.len() {
            let Some((b, _)) = self.struts[k].b else {
                continue;
            };
            let a = self.struts[k].a.0;
            if self.bodies[a].sleep.asleep != self.bodies[b].sleep.asleep {
                self.bodies[a].wake();
                self.bodies[b].wake();
                woke = true;
            }
        }
        woke
    }

    /// The struts renumbered to the bodies' new indices (`map`), those on a body removed
    /// dropped.
    pub(super) fn renumber_struts(&mut self, map: &[Option<usize>]) {
        let moved =
            |(body, node): (usize, NodeId)| map.get(body).copied().flatten().map(|b| (b, node));
        self.struts.retain_mut(|s| {
            let Some(a) = moved(s.a) else { return false };
            s.a = a;
            match s.b {
                Some(b) => moved(b).map(|b| s.b = Some(b)).is_some(),
                None => true,
            }
        });
    }
}
