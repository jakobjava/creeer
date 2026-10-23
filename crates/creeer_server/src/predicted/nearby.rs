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
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

/// A body's motion as predicted: position, orientation, velocity, spin.
pub(super) type Motion = (DVec3, DQuat, DVec3, DVec3);

fn motion(body: &Body) -> Motion {
    let s = &body.structure;
    (
        s.position[0],
        s.orientation[0],
        s.velocity[0],
        s.angular_velocity[0],
    )
}

/// Whether a predicted motion is the server's `state` within twice the snapshots' quanta.
fn agrees((x, q, v, w): Motion, state: &BodyState) -> bool {
    // Turned apart by θ, 1 − |q·r| ≈ θ²/8: within a milliradian (ten of the rotation's quanta).
    (x - state.position()).length() < 2.0 / POSITION_SCALE
        && (v - state.velocity()).length() < 2.0 / VELOCITY_SCALE
        && (w - state.spin()).length() < 2.0 / VELOCITY_SCALE
        && 1.0 - q.dot(state.orientation()).abs() < 1e-6 / 8.0
}

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

    /// Body `id` as predicted after input `seq`.
    fn at(&self, seq: u32, id: EntityId) -> Option<Motion> {
        let (_, _, motions) = self.entries.iter().find(|e| e.0 == seq)?;
        motions.iter().find(|m| m.0 == id).map(|m| m.1)
    }

    /// Whether the prediction after `snapshot`'s input had the avatar (`avatar`) and the
    /// bodies `ids` where the snapshot has them, within its quanta (the bodies it left out
    /// unknown, taken as agreeing). Then there is nothing to correct, and nothing to replay
    /// (as Chaos resimulates only on a misprediction).
    pub(super) fn agrees(&self, snapshot: &Snapshot, avatar: EntityId, ids: &[EntityId]) -> bool {
        let Some((_, mine, motions)) = (self.entries.iter()).find(|e| e.0 == snapshot.last_input)
        else {
            return false;
        };
        let Some(state) = snapshot.states.get(&avatar) else {
            return false;
        };
        agrees(*mine, state)
            && motions.len() == ids.len()
            && (motions.iter().zip(ids)).all(|((id, m), want)| {
                id == want
                    && (snapshot.stale.binary_search(id).is_ok()
                        || snapshot.states.get(id).is_some_and(|s| agrees(*m, s)))
            })
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

/// The predictable bodies near `at` in `snapshot`, nearest first.
pub(super) fn chosen<'a>(
    snapshot: &'a Snapshot,
    kinds: &'a BTreeMap<EntityId, Kind>,
    at: DVec3,
) -> Vec<(EntityId, &'a BodyState, &'a NetShape, &'a Matter)> {
    // (A little inside the server's radius: what it lists as stale reaches a little further.)
    let reach = 0.95 * NEARBY;
    let mut near: Vec<_> = (snapshot.states.iter())
        .filter_map(|(id, state)| match kinds.get(id) {
            Some(Kind::Body(shape, matter)) if matter.predicted => {
                let d = (state.position() - at).length();
                (d < reach).then_some((d, *id, state, shape, matter))
            }
            _ => None,
        })
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    near.truncate(MOST_NEARBY);
    near.into_iter()
        .map(|(_, id, s, sh, m)| (id, s, sh, m))
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
        let index = world.add(b);
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
