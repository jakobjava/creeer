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
            let [x, y, z] = a.extents();
            DVec3::new(x, y, z) * 0.5
        }
        Item::Bulk(b) => DVec3::splat(0.5 * creeer_math::det::cbrt(b.volume())),
    }
}

/// A word for what lies there (not what it is made of: principle 9).
fn word(item: &Item) -> &'static str {
    match item {
        Item::Piece(p, _) => match p.form {
            Form::Lump { .. } | Form::Ball { .. } => "stone",
            Form::Rod { .. } => "stick",
            Form::Block { size } if size.min_element() < 0.004 => "strip",
            Form::Block { .. } => "block",
        },
        Item::Made(_) => "tool",
        Item::Bulk(_) => "grit",
    }
}

impl ServerWorld {
    /// Lays `item` on the ground under `near`, turned by `turn`: its id.
    pub(super) fn lay(&mut self, item: Item, near: DVec3, turn: DQuat) -> u64 {
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
                    word: word(item).into(),
                })
            })
            .collect();
        (new, self.items.laid_log.len())
    }
}
