//! The bodies near the avatar, predicted with it (G1.2e, ADR-0025): at each reconciliation
//! the nearest predictable bodies within [`NEARBY`] of the avatar are put into the prediction
//! world as the server had them at the snapshot's tick (or, for one the snapshot left out, as
//! this client had predicted it then), and the inputs not yet applied are replayed on all of
//! them, as Unreal's resimulation bubble does. Walking into a crate is then predicted, not
//! corrected a round trip late.

use crate::world::DT;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::{EntityId, Kind, Matter, Shape as NetShape};
use creeer_net::predict::{MOST_NEARBY, NEARBY};
use creeer_net::quant::{BodyState, POSITION_SCALE, VELOCITY_SCALE};
use creeer_net::snapshot::Snapshot;
use creeer_physics::world::hull::Hull;
use creeer_physics::world::shape::Shape;
use creeer_physics::world::surface::Surface;
use creeer_physics::world::{Body, World};
use creeer_sim::character;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

/// A body's motion as predicted: position, orientation, velocity, spin.
pub(super) type Motion = (DVec3, DQuat, DVec3, DVec3);

pub(super) fn motion(body: &Body) -> Motion {
    let s = &body.structure;
    (
        s.position[0],
        s.orientation[0],
        s.velocity[0],
        s.angular_velocity[0],
    )
}

/// Whether a predicted motion is the server's `state` within twice the snapshots' quanta.
pub(super) fn agrees(m: Motion, state: &BodyState) -> bool {
    off_in(m, state) == [false; 4]
}

/// Which of a predicted motion's position, velocity, spin and orientation are off the server's
/// `state` by more than twice the snapshots' quanta.
pub(super) fn off_in((x, q, v, w): Motion, state: &BodyState) -> [bool; 4] {
    // Turned apart by θ, 1 − |q·r| ≈ θ²/8: within a milliradian (ten of the rotation's quanta).
    [
        (x - state.position()).length() >= 2.0 / POSITION_SCALE,
        (v - state.velocity()).length() >= 2.0 / VELOCITY_SCALE,
        (w - state.spin()).length() >= 2.0 / VELOCITY_SCALE,
        1.0 - q.dot(state.orientation()).abs() >= 1e-6 / 8.0,
    ]
}

/// How far a prediction was off a snapshot: the avatar's position and velocity (m, m/s), the
/// most of any body's, and which of position, velocity, spin and orientation were off
/// (`off_in`), the avatar's and any body's.
pub(super) type Off = ((f64, f64), (f64, f64), [[bool; 4]; 2]);

/// After an input (its number): the avatar's motion and each body's.
type Entry = (u32, Motion, Vec<(EntityId, Motion)>);

/// The avatar's and the bodies' motions after each input: the client's own prediction at a
/// snapshot's input, to compare the snapshot with (a rewind only where they differ) and for
/// the bodies a snapshot left out (`stale`).
#[derive(Debug, Default)]
pub(super) struct History {
    entries: VecDeque<Entry>,
}

impl History {
    /// After input `seq`, the avatar (body 0 of `world`) and the bodies `ids` (bodies 1..)
    /// moved so.
    pub(super) fn record(&mut self, seq: u32, ids: &[EntityId], world: &World) {
        let avatar = motion(&world.bodies[0]);
        let motions = ids
            .iter()
            .zip(&world.bodies[1..])
            .map(|(id, b)| (*id, motion(b)))
            .collect();
        match self.entries.iter_mut().find(|e| e.0 == seq) {
            Some(e) => (e.1, e.2) = (avatar, motions),
            None => self.entries.push_back((seq, avatar, motions)),
        }
        while self.entries.len() > creeer_net::predict::KEEP {
            self.entries.pop_front();
        }
    }

    /// How far the prediction after `snapshot`'s input was off it (`Off`), none where nothing
    /// was kept.
    pub(super) fn off(&self, snapshot: &Snapshot, avatar: EntityId) -> Option<Off> {
        let (_, mine, motions) = (self.entries.iter()).find(|e| e.0 == snapshot.last_input)?;
        let by = |(x, _, v, _): Motion, s: &BodyState| {
            ((x - s.position()).length(), (v - s.velocity()).length())
        };
        let state = snapshot.states.get(&avatar)?;
        let me = by(*mine, state);
        let mut which = [off_in(*mine, state), [false; 4]];
        let mut bodies = (0.0f64, 0.0f64);
        for (id, m) in motions {
            if let Some(s) = snapshot.states.get(id) {
                let b = by(*m, s);
                bodies = (bodies.0.max(b.0), bodies.1.max(b.1));
                for (k, off) in off_in(*m, s).into_iter().enumerate() {
                    which[1][k] |= off;
                }
            }
        }
        Some((me, bodies, which))
    }

    /// The avatar's and the bodies' motions as predicted after input `seq`.
    pub(super) fn after(&self, seq: u32) -> Option<(Motion, &[(EntityId, Motion)])> {
        let (_, mine, motions) = self.entries.iter().find(|e| e.0 == seq)?;
        Some((*mine, motions))
    }

    /// The newest input kept.
    pub(super) fn newest(&self) -> Option<u32> {
        self.entries.back().map(|e| e.0)
    }

    /// Every motion after input `seq` changed by `change` (the avatar's: `None`; a body's: its
    /// id), given the input it is after.
    pub(super) fn shift(
        &mut self,
        seq: u32,
        mut change: impl FnMut(Option<EntityId>, u32, Motion) -> Motion,
    ) {
        for (at, mine, motions) in self.entries.iter_mut().filter(|e| e.0 > seq) {
            *mine = change(None, *at, *mine);
            for (id, m) in motions.iter_mut() {
                *m = change(Some(*id), *at, *m);
            }
        }
    }

    /// Body `id` as predicted after input `seq`.
    fn at(&self, seq: u32, id: EntityId) -> Option<Motion> {
        let (_, _, motions) = self.entries.iter().find(|e| e.0 == seq)?;
        motions.iter().find(|m| m.0 == id).map(|m| m.1)
    }

    /// Whether the prediction after `snapshot`'s input had the avatar (`avatar`) and the
    /// bodies `ids` where the snapshot has them, within its quanta (the bodies it left out
    /// unknown, taken as agreeing). Then there is nothing to correct, and nothing to replay
    /// (as Chaos resimulates only on a misprediction).
    /// It does where this returns (true, 0): whether the avatar's prediction agrees with it, and
    /// how many of the bodies `ids` do not (all of them where nothing was kept for that input).
    pub(super) fn disagreeing(
        &self,
        snapshot: &Snapshot,
        avatar: EntityId,
        ids: &[EntityId],
    ) -> (bool, usize) {
        let Some((_, mine, motions)) = (self.entries.iter()).find(|e| e.0 == snapshot.last_input)
        else {
            return (false, ids.len());
        };
        let Some(state) = snapshot.states.get(&avatar) else {
            return (false, ids.len());
        };
        if motions.len() != ids.len() {
            return (agrees(*mine, state), ids.len());
        }
        let off = (motions.iter().zip(ids))
            .filter(|((id, m), want)| {
                id != *want
                    || !(snapshot.stale.binary_search(id).is_ok()
                        || snapshot.states.get(id).is_some_and(|s| agrees(*m, s)))
            })
            .count();
        (agrees(*mine, state), off)
    }

    /// Forgets what came before input `seq`.
    pub(super) fn forget_before(&mut self, seq: u32) {
        self.entries.retain(|e| e.0 >= seq);
    }
}

/// The body a client predicts for `shape` of `matter` (once each: kept and cloned).
fn body(shape: &NetShape, matter: &Matter) -> Option<Body> {
    let shape = match shape {
        NetShape::Box(half) => Shape::Box { half: *half },
        NetShape::Sphere(radius) => Shape::Sphere { radius: *radius },
        NetShape::Hull(corners) => Shape::Hull(Arc::new(Hull::new(corners).ok()?.0)),
    };
    let mut body = Body::rigid(shape, matter.mass, DVec3::ZERO, DT);
    body.friction = matter.friction;
    body.restitution = matter.restitution;
    let [e, nu, rounding] = matter.surface;
    body.surface = Surface {
        rounding,
        ..Surface::new(e, nu)
    };
    Some(body)
}

/// Room for what the motions do not show (m): a body set turning, pushed from outside.
const MARGIN: f64 = 0.5;
/// How much farther a body predicted now may go and still be predicted (m): one at the edge
/// would otherwise come and go at every snapshot, each a rewind (Unity's prediction switching
/// keeps such a margin).
const STAY: f64 = 1.0;
/// Ticks of motion past the inputs replayed that the prediction runs before it is corrected
/// again: to the next snapshot (one every two ticks) and one lost.
const AFTER: usize = 4;

/// What the avatar could reach before its next correction.
pub(super) struct Reach {
    at: DVec3,
    /// Its speed at most meanwhile (m/s).
    speed: f64,
    /// How long the prediction runs on from the snapshot (s).
    horizon: f64,
}

impl Reach {
    /// From the avatar's `state` at a snapshot, with `replayed` inputs to apply since: its
    /// speed grown by a jump at once and its jetpack's most thrust (the legs push less).
    pub(super) fn of(state: &BodyState, replayed: usize) -> Self {
        #[allow(clippy::cast_precision_loss)] // a few ticks
        let horizon = (replayed + AFTER) as f64 * DT;
        let surge = character::JUMP_SPEED + character::THRUST_MOST / character::MASS * horizon;
        Self {
            at: state.position(),
            speed: state.velocity().length() + surge,
            horizon,
        }
    }
}

/// A shape's reach about its centre (m): its bounding sphere's radius.
fn radius(shape: &NetShape) -> f64 {
    match shape {
        NetShape::Box(half) => half.length(),
        NetShape::Sphere(r) => *r,
        NetShape::Hull(corners) => corners.iter().map(|c| c.length()).fold(0.0, f64::max),
    }
}

/// The predictable bodies in `snapshot` the avatar could touch before its next correction
/// (`reach`), at once or through others it could touch: nearest first, at most
/// [`MOST_NEARBY`], within [`NEARBY`]. Each body is the sphere about it, reaching as far as it
/// moves in the time (one touched moving at least as fast as what touches it), and a margin.
/// Those predicted now (`kept`) stay while within [`STAY`] more. (Unreal's resimulation
/// bubble likewise takes only what the corrected motion could reach; every body predicted
/// costs its own step in each input replayed.)
pub(super) fn chosen<'a>(
    snapshot: &'a Snapshot,
    kinds: &'a BTreeMap<EntityId, Kind>,
    reach: &Reach,
    kept: &[EntityId],
) -> Vec<(EntityId, &'a BodyState, &'a NetShape, &'a Matter)> {
    // (A little inside the server's radius: what it lists as stale reaches a little further.)
    let within = 0.95 * NEARBY;
    let mut near: Vec<_> = (snapshot.states.iter())
        .filter_map(|(id, state)| match kinds.get(id) {
            Some(Kind::Body(shape, matter)) if matter.predicted => {
                let d = (state.position() - reach.at).length();
                (d < within).then_some((d, *id, state, shape, matter))
            }
            _ => None,
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    // Spheres that could touch: the avatar's (its box's), then each body one of them reaches.
    let mut touching = vec![(reach.at, character::torso().length(), reach.speed)];
    let mut picked = vec![false; near.len()];
    loop {
        let mut more = false;
        for (k, (_, id, state, shape, _)) in near.iter().enumerate() {
            if picked[k] {
                continue;
            }
            let (p, r, v) = (state.position(), radius(shape), state.velocity().length());
            let stay = if kept.contains(id) { STAY } else { 0.0 };
            let by = (touching.iter()).find(|&&(q, rq, s)| {
                (p - q).length() - rq - r <= (s + v) * reach.horizon + MARGIN + stay
            });
            if let Some(&(_, _, s)) = by {
                picked[k] = true;
                touching.push((p, r, v.max(s)));
                more = true;
            }
        }
        if !more {
            break;
        }
    }
    (near.into_iter().zip(picked))
        .filter(|(_, p)| *p)
        .take(MOST_NEARBY)
        .map(|((_, id, s, sh, m), _)| (id, s, sh, m))
        .collect()
}

/// The bodies kept to clone from, by entity.
#[derive(Debug, Default)]
pub(super) struct Templates(BTreeMap<EntityId, Body>);

impl Templates {
    /// Adds body `id` to `world` as the snapshot had it (`state`), or as `history` had it
    /// after input `seq` where the snapshot left it out (`stale`). False if its shape is not
    /// one a body can have.
    pub(super) fn add(
        &mut self,
        world: &mut World,
        (id, state, shape, matter): (EntityId, &BodyState, &NetShape, &Matter),
        (stale, history, seq): (bool, &History, u32),
    ) -> bool {
        let template = match self.0.get(&id) {
            Some(t) => t,
            None => {
                let Some(b) = body(shape, matter) else {
                    return false;
                };
                self.0.entry(id).or_insert(b)
            }
        };
        let mut b = template.clone();
        let own = if stale { history.at(seq, id) } else { None };
        let (x, q, v, w) = own.unwrap_or((
            state.position(),
            state.orientation(),
            state.velocity(),
            state.spin(),
        ));
        let s = &mut b.structure;
        (s.position[0], s.orientation[0]) = (x, q);
        (s.velocity[0], s.angular_velocity[0]) = (v, w);
        let index = world.add_copy(b);
        if state.asleep && own.is_none() {
            world.bodies[index].sleep_now();
        }
        true
    }

    /// Forgets the bodies no longer in view.
    pub(super) fn retain(&mut self, kinds: &BTreeMap<EntityId, Kind>) {
        self.0.retain(|id, _| kinds.contains_key(id));
    }
}
