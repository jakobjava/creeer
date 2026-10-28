//! The player's own load drawn where they steer it (G2.10, ADR-0036). It is drawn at the
//! server's pose relative to the avatar (`in_own_hands`), which answers the player's steering a
//! round trip late; so it is drawn moved on by the change of the hold since the input the
//! server had applied at the moment drawn (each snapshot says which: `Snapshot::last_input`).
//! The server's hands do not bring the load to a hold at once: they plan their way there,
//! critically damped at 3 Hz, as fast as their strength allows (`hands.rs`). So the part of the
//! change the server has applied is released as a model of that plan follows it, stepped a tick
//! at a time on the timeline drawn: where the server follows as the model does, the drawn load
//! stays where the player steered it and the correction ends without a jump. (Released at once,
//! it jumped back by the whole change and came on again with the server's.) Held steady, the
//! server holds a point in the world: the hold's changes then count for nothing.
//!
//! As the god-object of haptics (Zilles and Salisbury 1995) keeps the hand's intent apart from
//! what it holds, the correction leads the server's load by no more than its hands are yet to
//! do: one blocked (it did not follow) is drawn in what blocks it for the round trip and the
//! plan's settling, then where the server has it.

use super::Connection;
use crate::world::DT;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::{EntityId, Kind};
use creeer_net::interp::Pose;
use creeer_net::message::{AvatarInput, Hold};
use creeer_sim::carry;
use std::collections::{BTreeMap, VecDeque};

/// A gap this long in the timeline drawn (ticks) and the model starts afresh where it is meant.
const RESTART: i64 = 120;

/// The holds' changes summed to an input: the load's place from the avatar (m) and its turn
/// (a rotation vector, rad), each in the world's frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Sum {
    offset: DVec3,
    turn: DVec3,
}

/// The model of the server's hands' plan: its place and velocity, its turn and spin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Plan {
    at: DVec3,
    velocity: DVec3,
    turn: DVec3,
    spin: DVec3,
}

/// The steering of the player's own load, as the client draws it.
#[derive(Debug, Default)]
pub(crate) struct Steered {
    /// The load, its mass if known, one hand or two, and gravity there (m/s²).
    load: Option<(EntityId, Option<f64>, bool, f64)>,
    /// The holds' changes summed to each input since it was taken, oldest first.
    sums: VecDeque<(u32, Sum)>,
    /// The last input's hold.
    last: Option<Hold>,
    /// The plan at the tick drawn last stepped to, and a tick before.
    plan: Plan,
    before: Plan,
    tick: Option<i64>,
}

impl Steered {
    /// The summed change at input `x` (fractional, between inputs).
    fn sum_at(&self, x: f64) -> Sum {
        let mut below = self.sums.front().map_or(Sum::default(), |s| s.1);
        let mut below_seq = self.sums.front().map_or(0.0, |s| f64::from(s.0));
        for (seq, sum) in &self.sums {
            let seq = f64::from(*seq);
            if seq > x {
                let share = ((x - below_seq) / (seq - below_seq)).clamp(0.0, 1.0);
                return Sum {
                    offset: below.offset.lerp(sum.offset, share),
                    turn: below.turn.lerp(sum.turn, share),
                };
            }
            (below, below_seq) = (*sum, seq);
        }
        below
    }

    /// The plan one tick on towards `meant` (the server's law: `hands.rs`).
    fn step(&mut self, meant: Sum) {
        let spare = match self.load {
            Some((_, Some(mass), one_hand, g)) => carry::plan_spare(mass, g, one_hand),
            _ => f64::INFINITY,
        };
        let w = std::f64::consts::TAU * carry::PLAN_HZ;
        let p = &mut self.plan;
        let a = ((meant.offset - p.at) * (w * w) - p.velocity * (2.0 * w)).clamp_length_max(spare);
        p.velocity = (p.velocity + a * DT).clamp_length_max(carry::FASTEST);
        p.at += p.velocity * DT;
        let alpha = (meant.turn - p.turn) * (w * w) - p.spin * (2.0 * w);
        p.spin = (p.spin + alpha * DT).clamp_length_max(carry::TURNING_AT_MOST);
        p.turn += p.spin * DT;
    }
}

impl Connection {
    /// An input made: its hold's change from the last (none held steady, or to or from it).
    pub(super) fn steered_input(&mut self, input: &AvatarInput) {
        let load = self.own_load();
        if load != self.steered.load.map(|l| l.0) {
            // Taken anew (or let go): the model starts from here.
            let mass = load.and_then(|l| match self.kinds.get(&l) {
                Some(Kind::Body(_, matter)) => Some(matter.mass),
                _ => None,
            });
            let one_hand = matches!(
                self.welcome.as_ref().and_then(|w| self.kinds.get(&w.avatar)),
                Some(Kind::Character { carries: Some(c), .. }) if c.one_hand
            );
            // Gravity where the avatar is (the server's hands hold up the load's weight).
            let g = (self.terrain.as_ref().zip(self.avatar())).map_or(9.806_65, |(t, a)| {
                creeer_sim::planet_gravity(t.planet())
                    .acceleration(a.position)
                    .length()
            });
            let s = &mut self.steered;
            *s = Steered::default();
            s.load = load.map(|l| (l, mass, one_hand, g));
            s.sums.push_back((input.seq, Sum::default()));
            s.last = input.hold;
            return;
        }
        let s = &mut self.steered;
        let change = match (s.last, input.hold) {
            (Some(a), Some(b)) if !a.steady && !b.steady => Sum {
                offset: b.offset - a.offset,
                turn: (b.turn * a.turn.conjugate()).normalize().to_scaled_axis(),
            },
            _ => Sum::default(),
        };
        let last = s.sums.back().map_or(Sum::default(), |l| l.1);
        s.sums.push_back((
            input.seq,
            Sum {
                offset: last.offset + change.offset,
                turn: last.turn + change.turn,
            },
        ));
        s.last = input.hold;
    }

    /// The input the server had applied at tick `tick` of the timeline drawn (fractional: one a
    /// tick between the snapshots that say).
    fn applied_at(&self, tick: f64) -> f64 {
        let mut before: Option<(f64, f64)> = None;
        for s in self.receiver().snapshots() {
            let (t, l) = (f64::from(s.tick), f64::from(s.last_input));
            if t > tick {
                return match before {
                    Some((t0, l0)) => l0 + (l - l0) * (tick - t0) / (t - t0),
                    None => l,
                };
            }
            before = Some((t, l));
        }
        before.map_or(0.0, |(t, l)| l + (tick - t).max(0.0))
    }

    /// The timeline drawn moved on: the model of the server's hands stepped to it, following
    /// the holds the server had applied there.
    pub(super) fn steered_advance(&mut self) {
        if self.steered.load.is_none() {
            return;
        }
        #[allow(clippy::cast_possible_truncation)] // ticks, far within an i64
        let now = self.drawn_at.floor() as i64;
        let from = match self.steered.tick {
            Some(t) if now - t <= RESTART => t,
            _ => {
                // Afresh: the plan where the server's hands mean it now.
                #[allow(clippy::cast_precision_loss)]
                let meant = self.steered.sum_at(self.applied_at(now as f64));
                let plan = Plan {
                    at: meant.offset,
                    turn: meant.turn,
                    ..Plan::default()
                };
                (self.steered.plan, self.steered.before) = (plan, plan);
                now
            }
        };
        for tick in from + 1..=now {
            #[allow(clippy::cast_precision_loss)]
            let meant = self.steered.sum_at(self.applied_at(tick as f64));
            self.steered.before = self.steered.plan;
            self.steered.step(meant);
        }
        self.steered.tick = Some(now);
        // What the server has applied for good is forgotten (one kept to step from).
        let applied = self.applied_at(self.drawn_at).floor() - 1.0;
        let s = &mut self.steered;
        while s.sums.get(1).is_some_and(|n| f64::from(n.0) <= applied) {
            s.sums.pop_front();
        }
    }

    /// The player's own load in `poses` moved on by what the player has steered it that the
    /// server's hands have not yet shown (`Steered`).
    pub(super) fn steered_drawn(&self, poses: &mut BTreeMap<EntityId, Pose>) {
        let s = &self.steered;
        let (Some((load, ..)), Some(tick)) = (s.load, s.tick) else {
            return;
        };
        let Some(pose) = poses.get_mut(&load) else {
            return;
        };
        #[allow(clippy::cast_precision_loss)]
        let share = (self.drawn_at - tick as f64).clamp(0.0, 1.0);
        let total = s.sums.back().map_or(Sum::default(), |l| l.1);
        let at = s.before.at.lerp(s.plan.at, share);
        let turn = s.before.turn.lerp(s.plan.turn, share);
        pose.position += total.offset - at;
        pose.orientation =
            (DQuat::from_scaled_axis(total.turn - turn) * pose.orientation).normalize();
    }

    /// How far the player's own load is drawn from where the server has it (m, the world's
    /// frame; rad), as it is drawn now (for tests and diagnostics).
    pub fn steered_by(&self) -> (DVec3, f64) {
        let mut poses = BTreeMap::new();
        let Some((load, ..)) = self.steered.load else {
            return (DVec3::ZERO, 0.0);
        };
        let at = Pose {
            position: DVec3::ZERO,
            orientation: DQuat::IDENTITY,
        };
        poses.insert(load, at);
        self.steered_drawn(&mut poses);
        let p = poses[&load];
        (p.position, p.orientation.angle_between(DQuat::IDENTITY))
    }

    /// The change of the hold's place since the input the server had applied at the moment
    /// drawn (m): the correction were it released at once, as the server applies it (rejected:
    /// ADR-0036; for tests).
    pub fn steered_unapplied(&self) -> DVec3 {
        let s = &self.steered;
        let total = s.sums.back().map_or(Sum::default(), |l| l.1);
        total.offset - s.sum_at(self.applied_at(self.drawn_at)).offset
    }

    /// The newest input made with the load in hand, and the input the server had applied at the
    /// moment drawn (fractional): their difference is the round trip as the player sees it.
    pub fn steered_inputs(&self) -> (u32, f64) {
        let newest = self.steered.sums.back().map_or(0, |s| s.0);
        (newest, self.applied_at(self.drawn_at))
    }
}
