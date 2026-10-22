//! Making by hand in the world (G1.6d, ADR-0030): a blow breaks the brittle item it lands on
//! (`creeer_sim::making::knap`), a made thing whose binding a blow overcomes comes apart, bark
//! is stripped from the branch in the hands, and a head is bound to the haft in the hands with
//! a strip from a pocket. Each is checked and done whole.

use super::*;
use creeer_net::message::Target;
use creeer_sim::items::{Form, Item, Piece};
use creeer_sim::making::{self, Broken};

/// How long a blow's contact lasts (s): a few milliseconds for stone or steel on wood or stone,
/// so its peak force is about its impulse over this.
const CONTACT: f64 = 0.002;
/// The bast stripped from wood: its material, as the packs have it.
const BAST: &str = "spruce_bast";

impl ServerWorld {
    /// Avatar `by`'s blow of `energy` (J) and impulse `j` (N·s) fell on body `on`: a brittle item
    /// there breaks; the made thing that struck, if its binding gives, comes apart.
    pub(super) fn broken_by(&mut self, by: EntityId, on: EntityId, energy: f64, j: f64) {
        self.break_item(on, energy);
        let Some(held) = self.carried(by) else { return };
        let Some(Item::Made(made)) = self.items.bodies.get(&held) else {
            return;
        };
        if j / CONTACT <= made.hold {
            return;
        }
        let at = self.hands_point(by);
        self.let_go(by);
        if let Some(Item::Made(made)) = self.remove_item(held) {
            for (k, piece) in making::apart(*made).into_iter().enumerate() {
                #[allow(clippy::cast_precision_loss)] // a few
                let up = at.normalize() * (0.1 * k as f64);
                self.add_item(piece, at + up, true);
            }
        }
    }

    /// Item body `id`, if a brittle piece, struck with `energy` (J): what breaks off lies beside it.
    fn break_item(&mut self, id: EntityId, energy: f64) {
        let Some(Item::Piece(piece, inside)) = self.items.bodies.get(&id) else {
            return;
        };
        if !inside.is_empty() {
            return;
        }
        let Some(breaks) = self
            .items
            .densities
            .as_ref()
            .and_then(|d| d.breaks(&piece.material))
        else {
            return;
        };
        let mut core = piece.clone();
        let Some(broken) = making::knap(&mut core, &breaks, energy) else {
            return;
        };
        let Some(i) = self.index(id) else { return };
        let s = &self.world.bodies[i].structure;
        let (at, turn) = (s.position[0], s.orientation[0]);
        self.remove_item(id);
        let new = self.add_item(Item::Piece(core, Vec::new()), at, false);
        if let Some(k) = self.index(new) {
            let b = &mut self.world.bodies[k];
            b.structure.orientation[0] = turn;
            b.solver.reset_history(&b.structure);
        }
        let off = match broken {
            Broken::Flake(p) => Item::Piece(p, Vec::new()),
            Broken::Grit(g) => Item::Bulk(g),
        };
        let up = character::plumb(&self.world, at);
        let (side, _) = creeer_terrain::geology::deposits::tangent_frame(up);
        self.add_item(off, at + side * 0.12 + up * 0.05, true);
    }

    /// Avatar `by` strips bark from the branch in its hands: the strip into the first pocket it
    /// fits, or down beside.
    pub(super) fn strip(&mut self, by: EntityId) -> Result<(), String> {
        let held = self.carried(by).ok_or("nothing in hand")?;
        let Some(Item::Piece(branch, _)) = self.items.bodies.get(&held) else {
            return Err("no branch in hand".into());
        };
        let densities = self.items.densities.as_ref().ok_or("no materials")?;
        let bast = densities.of(BAST).ok_or("no bast in the packs")?;
        let mut branch = branch.clone();
        let strip = making::strip(&mut branch, (BAST, bast)).ok_or("no bark peels from it")?;
        self.swap_held(by, Item::Piece(branch, Vec::new()))?;
        let strip = Item::Piece(strip, Vec::new());
        let at = self.hands_point(by);
        let pockets = self.items.pockets.get(&by).unwrap_or(&self.items.outfit);
        match pockets.iter().position(|p| p.refuses(&strip).is_none()) {
            Some(k) => self
                .change_pockets(by, |p| p[k].put(strip))
                .map_err(|refused| refused.1),
            None => {
                self.add_item(strip, at + at.normalize() * 0.2, true);
                Ok(())
            }
        }
    }

    /// Avatar `by` binds the head it reaches to the haft in its hands with the first strip in its
    /// pockets.
    pub(super) fn bind(&mut self, by: EntityId, target: &Target) -> Result<(), String> {
        let held = self.carried(by).ok_or("no haft in hand")?;
        let Some(Item::Piece(haft, _)) = self.items.bodies.get(&held) else {
            return Err("no haft in hand".into());
        };
        let head_id = target.body.ok_or("nothing to bind on")?;
        self.reached(by, target.body, target.at, target.seen)?;
        let Some(Item::Piece(head, _)) = self.items.bodies.get(&head_id) else {
            return Err("not a head".into());
        };
        let (pocket, k) = self
            .first_strip(by)
            .ok_or("no strip in the pockets to bind with")?;
        let densities = self.items.densities.as_ref().ok_or("no materials")?;
        let Item::Piece(binding, _) = &self.items.pockets[&by][pocket].items[k] else {
            unreachable!("a strip is a piece");
        };
        let strength = densities
            .strength(&binding.material)
            .ok_or("a binding of what?")?;
        let made = making::bind(haft.clone(), head.clone(), binding.clone(), strength)?;
        self.change_pockets(by, |p| p[pocket].take(k));
        self.remove_item(head_id);
        self.swap_held(by, Item::Made(Box::new(made)))
    }

    /// The first strip (a thin block) in avatar `by`'s pockets: its pocket and place.
    fn first_strip(&self, by: EntityId) -> Option<(usize, usize)> {
        let thin = |i: &Item| {
            matches!(i, Item::Piece(Piece { form: Form::Block { size }, .. }, _)
                if size.min_element() * 10.0 < size.max_element())
        };
        let pockets = self.items.pockets.get(&by)?;
        (pockets.iter().enumerate())
            .find_map(|(p, s)| s.items.iter().position(thin).map(|k| (p, k)))
    }

    /// What avatar `by` holds, made into `item`: the old put down, the new taken in the hands.
    fn swap_held(&mut self, by: EntityId, item: Item) -> Result<(), String> {
        let held = self.carried(by).ok_or("nothing in hand")?;
        self.let_go(by);
        self.remove_item(held);
        self.put_in_hands(by, item)
    }
}
