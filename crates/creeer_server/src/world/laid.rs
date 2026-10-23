//! Small things at rest are scenery (G1.6, ADR-0030 amended): an item that lies still on the
//! ground (a flake, a pebble, a stick, the core a blow left) is no physics body but laid where it
//! lies, as the scatter's stones are: no cost per tick, drawn and aimed at by clients, picked up
//! and struck like them. What breaks off in a blow is laid at once, and a small item body that
//! comes to rest is laid. (As bodies, centimetre flakes settling on the terrain took 50–190
//! Newton iterations a tick each, and a few of them froze the game.)

use super::*;
use creeer_math::glam::DQuat;
use creeer_net::message::Laid;
use creeer_sim::items::{Form, Item};

/// The longest an item laid at rest may be (m): larger ones stay bodies, to be walked into.
pub const LAID_LARGEST: f64 = 0.3;
/// Laid items' ids are apart from the scatter's (which use 49 bits).
const LAID: u64 = 1 << 63;

/// An item's box in its own frame: its half extents.
pub(super) fn half(item: &Item) -> DVec3 {
    match item {
        Item::Piece(p, _) => match p.form {
            Form::Block { size } | Form::Lump { size } => size * 0.5,
            Form::Ball { radius } => DVec3::splat(radius),
            Form::Rod { radius, length } => DVec3::new(length * 0.5, radius, radius),
        },
        Item::Made(a) => {
            let (lo, hi) = a.bounds();
            (hi - lo) * 0.5
        }
        Item::Bulk(b) => DVec3::splat(0.5 * creeer_math::det::cbrt(b.volume())),
    }
}

impl ServerWorld {
    /// Lays `item` on the ground under `near`, turned by `turn`: its id.
    pub(crate) fn lay(&mut self, item: Item, near: DVec3, turn: DQuat) -> u64 {
        let ground = self.ground_under(near);
        let up = character::plumb(&self.world, ground);
        let h = half(&item);
        let local = turn.conjugate() * up;
        let height = local.abs().dot(h);
        let id = LAID | self.items.next_laid;
        self.items.next_laid += 1;
        self.items
            .laid
            .insert(id, (item, ground + up * height, turn));
        self.items.laid_log.push(id);
        id
    }

    /// Item body `id` laid where it is, as it is turned.
    pub(super) fn lay_body(&mut self, id: EntityId) {
        let Some(i) = self.index(id) else { return };
        let s = &self.world.bodies[i].structure;
        let (at, turn) = (s.position[0], s.orientation[0]);
        if let Some(item) = self.remove_item(id) {
            self.lay(item, at, turn);
        }
    }

    /// After the step: every small item body that has come to rest, and nobody holds, is laid.
    pub(super) fn lay_resting(&mut self) {
        let resting: Vec<EntityId> = (self.items.bodies.iter())
            .filter(|(id, item)| {
                let small = item.extents().is_some_and(|e| e[0] <= LAID_LARGEST);
                let still = (self.index(**id)).is_some_and(|i| self.world.bodies[i].sleep.asleep);
                small && still && !self.in_hands(**id) && !self.aimed_at(**id)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in resting {
            self.lay_body(id);
        }
    }

    /// The items laid since the first `told` the log holds, as clients draw them, and how long
    /// the log is now.
    pub fn laid_since(&self, told: usize) -> (Vec<Laid>, usize) {
        let new = (self.items.laid_log.get(told..).unwrap_or_default().iter())
            .filter_map(|id| {
                let (item, centre, turn) = self.items.laid.get(id)?;
                let colour = (item.head())
                    .and_then(|h| self.items.densities.as_ref()?.colour(&h.material))
                    .unwrap_or([128, 120, 110]);
                Some(Laid {
                    id: *id,
                    centre: *centre,
                    turn: *turn,
                    half: half(item),
                    colour,
                    word: creeer_sim::items::kit::word(item).into(),
                    mass: item.mass(),
                    parts: match item {
                        Item::Made(a) => {
                            let (lo, hi) = a.bounds();
                            let d = self.items.densities.as_ref();
                            super::item_bodies::parts(a, (lo + hi) * 0.5, |m| d?.colour(m))
                        }
                        _ => Vec::new(),
                    },
                })
            })
            .collect();
        (new, self.items.laid_log.len())
    }
}

impl ServerWorld {
    /// What item body `id` is, if it is one: a word for it, and the pieces it is drawn as if it
    /// is made of pieces (about its centre), and its mass (kg).
    pub fn item_look(&self, id: EntityId) -> Option<(String, Vec<creeer_net::message::Part>, f64)> {
        let item = self.items.bodies.get(&id)?;
        let word = creeer_sim::items::kit::word(item).to_owned();
        let mass = item.mass();
        let Item::Made(a) = item else {
            return Some((word, Vec::new(), mass));
        };
        let (_, centroid) = super::item_bodies::made_hull(a);
        let d = self.items.densities.as_ref();
        Some((
            word,
            super::item_bodies::parts(a, centroid, |m| d?.colour(m)),
            mass,
        ))
    }

    /// The pieces item body `id` is drawn as, if it is made of pieces.
    pub fn drawn_parts(&self, id: EntityId) -> Option<Vec<creeer_net::message::Part>> {
        self.item_look(id).map(|l| l.1).filter(|p| !p.is_empty())
    }

    /// Tells avatar `by`'s player what came of what they did.
    pub(super) fn note(&mut self, by: EntityId, note: String) {
        self.items.notes.push((by, note));
    }

    /// The notes since the last call: for whom, and what.
    pub fn take_notes(&mut self) -> Vec<(EntityId, String)> {
        std::mem::take(&mut self.items.notes)
    }
}
