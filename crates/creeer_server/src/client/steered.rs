//! The player's own load drawn where the server's hands will bring it (G2.10, ADR-0036). The
//! server draws it at its pose a round trip after the player aimed it; so the client steps the
//! hands' law for aimed things (`carry::aimed_step`: straight to the hold, as fast as hands
//! move and turn) on each input it sends, as the server will when the input arrives, and draws
//! the load moved on by how far that law took it from the input the server had applied at the
//! moment drawn (each snapshot says which: `Snapshot::last_input`) to the newest. Where the
//! server follows the law, the drawn load glides as the server's will, a round trip early, and
//! comes to the server's without a jump.
//!
//! (Drawn at the newest hold at once, the load jumped there and the server's came on behind at
//! the hands' speed, drawn up to 2.5 m apart as the view swept from the ground to the sky; and a
//! model of a damped plan the server no longer followed for aimed holds drew it astray, G2.10.)
//! Held steady (fastening), the server holds a point in the world: the hold's changes then count
//! for nothing. As the god-object of haptics (Zilles and Salisbury 1995) keeps the hand's intent
//! apart from what it holds, a load blocked (it did not follow) is drawn in what blocks it for
//! the round trip and its glide, then where the server has it.

use super::Connection;
use crate::world::DT;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::interp::Pose;
use creeer_net::message::{AvatarInput, Hold};
use creeer_sim::carry;
use std::collections::{BTreeMap, VecDeque};

/// Where the hands' law has brought the load after an input: its place from the avatar (m) and
/// its turn, in the world's frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Follow {
    offset: DVec3,
    turn: DQuat,
}

/// The steering of the player's own load, as the client draws it.
#[derive(Debug, Default)]
pub(crate) struct Steered {
    /// The load.
    load: Option<EntityId>,
    /// The holds of the inputs made since it was taken that the law has not yet been stepped
    /// on: until the server has shown one applied (`steered_advance`), the law starts there.
    pending: VecDeque<(u32, Option<Hold>)>,
    /// Where the law brought it after each input, oldest first.
    follows: VecDeque<(u32, Follow)>,
    /// Where the hands grip it, in its frame (as the server last told: `Carried::grip`).
    grip: DVec3,
}

/// The law stepped on from `from` towards `hold` (none, or held steady: not moved), at the point
/// the hands grip (`grip`, in the load's frame), as the server's hands bring it: turned there,
/// a load held off its middle swings its middle on another path than its grip's.
fn stepped(from: Follow, hold: Option<Hold>, grip: DVec3) -> Follow {
    match hold {
        Some(h) if !h.steady => {
            let at = from.offset + from.turn * grip;
            let to = h.offset + h.turn * grip;
            let (at, turn, ..) = carry::aimed_step((at, from.turn), (to, h.turn), DT);
            Follow {
                offset: at - turn * grip,
                turn,
            }
        }
        _ => from,
    }
}

impl Steered {
    /// Where the law had it at input `x` (fractional, between inputs).
    fn at(&self, x: f64) -> Follow {
        let mut below = self.follows.front().map_or(Follow::default(), |s| s.1);
        let mut below_seq = self.follows.front().map_or(0.0, |s| f64::from(s.0));
        for (seq, f) in &self.follows {
            let seq = f64::from(*seq);
            if seq > x {
                let share = ((x - below_seq) / (seq - below_seq)).clamp(0.0, 1.0);
                return Follow {
                    offset: below.offset.lerp(f.offset, share),
                    turn: below.turn.slerp(f.turn, share),
                };
            }
            (below, below_seq) = (*f, seq);
        }
        below
    }

    /// Where the law has it after the newest input.
    fn newest(&self) -> Follow {
        self.follows.back().map_or(Follow::default(), |f| f.1)
    }
}

impl Connection {
    /// An input made: the law stepped on towards its hold (none, or held steady: not moved).
    pub(super) fn steered_input(&mut self, input: &AvatarInput) {
        let load = self.own_load();
        if load != self.steered.load {
            // Taken anew (or let go): the law starts afresh.
            self.steered = Steered {
                load,
                ..Steered::default()
            };
        }
        // (Where the server last told the hands grip it: aimed, at its balance point.)
        self.steered.grip = self.own_carried().map_or(DVec3::ZERO, |c| c.grip);
        let s = &mut self.steered;
        if input.hold.is_none() {
            // Held by the hands themselves (not aimed): the server's, drawn as it is; aimed
            // again, the law starts afresh where the server has it.
            s.follows.clear();
            s.pending.clear();
            return;
        }
        match s.follows.back() {
            Some(&(_, last)) => s
                .follows
                .push_back((input.seq, stepped(last, input.hold, s.grip))),
            None => s.pending.push_back((input.seq, input.hold)),
        }
    }

    /// The law started where the server had the load when it had applied an input (as drawn
    /// now, before any correction), and stepped on by the newer inputs' holds. (Started where it
    /// was drawn when taken, a round trip before the server's hands had it there, the law glided
    /// 0.2 m behind the server's to the same hold, G2.10.)
    fn steered_start(&mut self) {
        let applied = self.applied_at(self.drawn_at).floor();
        let s = &self.steered;
        let (Some(load), true) = (s.load, s.follows.is_empty()) else {
            return;
        };
        if !s.pending.front().is_some_and(|p| f64::from(p.0) <= applied) {
            return;
        }
        let (Some(me), Some(drawn)) = (self.avatar(), self.poses().get(&load).copied()) else {
            return;
        };
        let start = Follow {
            offset: drawn.position - me.position,
            turn: drawn.orientation,
        };
        let s = &mut self.steered;
        while s.pending.get(1).is_some_and(|p| f64::from(p.0) <= applied) {
            s.pending.pop_front();
        }
        let Some((seq, _)) = s.pending.pop_front() else {
            return;
        };
        s.follows.push_back((seq, start));
        while let Some((seq, hold)) = s.pending.pop_front() {
            let last = s.follows.back().map_or(start, |f| f.1);
            s.follows.push_back((seq, stepped(last, hold, s.grip)));
        }
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

    /// The timeline drawn moved on: what the server has applied for good is forgotten (one
    /// kept to interpolate from).
    pub(super) fn steered_advance(&mut self) {
        self.steered_start();
        let applied = self.applied_at(self.drawn_at).floor() - 1.0;
        let s = &mut self.steered;
        while s.follows.get(1).is_some_and(|n| f64::from(n.0) <= applied) {
            s.follows.pop_front();
        }
    }

    /// The player's own load in `poses` moved on by how far the hands' law takes it that the
    /// server has not yet shown (`Steered`).
    pub(super) fn steered_drawn(&self, poses: &mut BTreeMap<EntityId, Pose>) {
        let s = &self.steered;
        let Some(load) = s.load else { return };
        if s.follows.is_empty() {
            return;
        }
        let Some(pose) = poses.get_mut(&load) else {
            return;
        };
        let (newest, shown) = (s.newest(), s.at(self.applied_at(self.drawn_at)));
        pose.position += newest.offset - shown.offset;
        pose.orientation = (newest.turn * shown.turn.conjugate() * pose.orientation).normalize();
    }

    /// How far the player's own load is drawn from where the server has it (m, the world's
    /// frame; rad), as it is drawn now (for tests and diagnostics).
    pub fn steered_by(&self) -> (DVec3, f64) {
        let mut poses = BTreeMap::new();
        let Some(load) = self.steered.load else {
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

    /// The newest input made with the load in hand, and the input the server had applied at the
    /// moment drawn (fractional): their difference is the round trip as the player sees it.
    pub fn steered_inputs(&self) -> (u32, f64) {
        let newest = self.steered.follows.back().map_or(0, |s| s.0);
        (newest, self.applied_at(self.drawn_at))
    }
}
