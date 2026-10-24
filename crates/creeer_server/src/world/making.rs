//! Making by hand in the world (G1.6d, ADR-0030): a blow breaks the brittle item it lands on
//! (`creeer_sim::making::knap`), a made thing whose binding a blow overcomes comes apart, bark
//! is stripped from the branch in the hands, and a head is bound to the haft in the hands with
//! a strip. Each is checked, done whole, and told to the player (what came of it, or why not).

use super::*;
use creeer_math::glam::DQuat;
use creeer_sim::items::kit::describe;
use creeer_sim::items::{Form, Item, Piece};
use creeer_sim::making::{self, Broken};

/// How long a blow's contact lasts (s): a few milliseconds for stone or steel on wood or stone,
/// so its peak force is about its impulse over this.
pub(super) const CONTACT: f64 = 0.002;
/// The bast stripped from wood: its material, as the packs have it.
const BAST: &str = "spruce_bast";
/// The shortest haft (m).
const SHORTEST_HAFT: f64 = 0.25;

/// What a head to bind is: an item body, or a thing lying on the ground (by its id).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Head {
    Body(EntityId),
    Lying(u64),
}

impl ServerWorld {
    /// Avatar `by`'s blow of `energy` (J) and impulse `j` (N·s) fell on body `on`: a brittle item
    /// there breaks; the made thing that struck, if its binding gives, comes apart.
    pub(super) fn broken_by(&mut self, by: EntityId, on: EntityId, energy: f64, j: f64) {
        self.break_item(by, on, energy);
        self.binding_bears(by, j / CONTACT);
    }

    /// A blow whose peak force on the head is `force` (N) by what avatar `by` holds: if a made
    /// thing's binding gives, it comes apart.
    pub(super) fn binding_bears(&mut self, by: EntityId, force: f64) {
        let Some(held) = self.carried(by) else { return };
        let Some(Item::Made(made)) = self.items.bodies.get(&held) else {
            return;
        };
        if force <= made.hold {
            return;
        }
        let why = format!(
            "the binding gave: a blow of {:.1} kN on its {:.1} kN",
            force / 1e3,
            made.hold / 1e3
        );
        let at = self.hands_point(by);
        self.let_go(by);
        if let Some(Item::Made(made)) = self.remove_item(held) {
            let up = character::plumb(&self.world, at);
            let (side, _) = creeer_terrain::geology::deposits::tangent_frame(up);
            for (k, piece) in making::apart(*made).into_iter().enumerate() {
                #[allow(clippy::cast_precision_loss)] // a few
                let along = side * (0.15 * k as f64);
                self.lay(piece, at + along, DQuat::from_rotation_arc(DVec3::Z, up));
            }
        }
        self.note(by, format!("{why}: it came apart"));
    }

    /// Item body `id`, if a brittle piece, struck by avatar `by` with `energy` (J): what breaks
    /// off lies beside it.
    fn break_item(&mut self, by: EntityId, id: EntityId, energy: f64) {
        let Some(Item::Piece(piece, inside)) = self.items.bodies.get(&id) else {
            return;
        };
        let breaks = (self.items.densities.as_ref()).and_then(|d| d.breaks(&piece.material));
        let Some(breaks) = breaks.filter(|_| inside.is_empty()) else {
            return;
        };
        let mut core = piece.clone();
        let Some(broken) = making::knap(&mut core, &breaks, energy) else {
            self.note(by, format!("it only bruised (a blow of {energy:.0} J)"));
            return;
        };
        let Some(i) = self.index(id) else { return };
        let s = &self.world.bodies[i].structure;
        let (at, turn) = (s.position[0], s.orientation[0]);
        // The core and what broke off both lie still: laid, not bodies (`laid.rs`).
        self.remove_item(id);
        let core = Item::Piece(core, Vec::new());
        if core
            .extents()
            .is_some_and(|e| e[0] <= super::laid::LAID_LARGEST)
        {
            self.lay(core, at, turn);
        } else {
            let new = self.add_item(core, at, false);
            if let Some(k) = self.index(new) {
                let b = &mut self.world.bodies[k];
                b.structure.orientation[0] = turn;
                b.solver.reset_history(&b.structure);
            }
        }
        let (off, note) = match broken {
            Broken::Flake(p) => {
                let flake = Item::Piece(p, Vec::new());
                let note = format!("a flake came off: {}", describe(&flake));
                (flake, note)
            }
            Broken::Grit(g) => {
                let note = format!("it crumbled: {:.0} g of grit came off", g.mass * 1000.0);
                (Item::Bulk(g), note)
            }
        };
        let up = character::plumb(&self.world, at);
        let (side, _) = creeer_terrain::geology::deposits::tangent_frame(up);
        self.lay(
            off,
            at + side * 0.12,
            DQuat::from_rotation_arc(DVec3::Z, up),
        );
        self.note(by, note);
    }

    /// Avatar `by` strips bark from the branch in its hands: the strip into the first pocket it
    /// fits, or down beside.
    pub(super) fn strip(&mut self, by: EntityId) -> Result<(), String> {
        let held = self.carried(by).ok_or("nothing in the hands to strip")?;
        let Some(Item::Piece(branch, _)) = self.items.bodies.get(&held) else {
            return Err("bark strips from a branch: hold one".into());
        };
        let densities = self.items.densities.as_ref().ok_or("no materials")?;
        let bast = densities.of(BAST).ok_or("no bast in the packs")?;
        let mut branch = branch.clone();
        let strip = (making::strip(&mut branch, (BAST, bast)))
            .ok_or("no bark peels from it: it is no branch, or thinner than 12 mm")?;
        self.swap_held(by, Item::Piece(branch, Vec::new()))?;
        let strip = Item::Piece(strip, Vec::new());
        let what = describe(&strip);
        let at = self.hands_point(by);
        let pockets = self.items.pockets.get(&by).unwrap_or(&self.items.outfit);
        match pockets.iter().position(|p| p.refuses(&strip).is_none()) {
            Some(k) => {
                let name = pockets[k].name.clone();
                self.change_pockets(by, |p| p[k].put(strip))
                    .map_err(|refused| refused.1)?;
                self.note(by, format!("a strip of bark into {name}: {what}"));
            }
            None => {
                let up = character::plumb(&self.world, at);
                self.lay(strip, at, DQuat::from_rotation_arc(DVec3::Z, up));
                self.note(
                    by,
                    format!("a strip of bark, laid down (no pocket takes it): {what}"),
                );
            }
        }
        Ok(())
    }

    /// Avatar `by` binds `head` (in reach) to the haft in its hands, with the first strip in its
    /// pockets or lying within reach.
    pub(super) fn bind(&mut self, by: EntityId, head: Head) -> Result<(), String> {
        let held = self
            .carried(by)
            .ok_or("hold a haft (a stick) to bind a head on")?;
        let haft = match self.items.bodies.get(&held) {
            Some(Item::Piece(p, _)) if matches!(p.form, Form::Rod { length, .. } if length >= SHORTEST_HAFT) => {
                p.clone()
            }
            _ => return Err(format!("a haft is a stick of {SHORTEST_HAFT} m or more")),
        };
        let head_item = match head {
            Head::Body(id) => self.items.bodies.get(&id).cloned().ok_or("nothing there")?,
            Head::Lying(id) => self.lying_in_reach(by, id)?.0,
        };
        let Item::Piece(stone, _) = &head_item else {
            return Err("a head is a stone".into());
        };
        let densities = self.items.densities.as_ref().ok_or("no materials")?;
        let is_stone = densities.breaks(&stone.material).is_some() && !is_strip(stone);
        if !is_stone {
            return Err("a head is a stone".into());
        }
        let (binding, from) = self.find_strip(by).ok_or(
            "no bark strip to bind with, in a pocket or lying in reach (R strips a branch)",
        )?;
        let strength = densities
            .strength(&binding.material)
            .ok_or("a binding of what?")?;
        let across = densities.across(&haft.material);
        let made = making::bind_split(haft, stone.clone(), binding, strength, across)?;
        match from {
            StripAt::Pocket(pocket, k) => {
                self.change_pockets(by, |p| p[pocket].take(k));
            }
            StripAt::Lying(id) => self.take_lying(id),
        }
        match head {
            Head::Body(id) => {
                self.remove_item(id);
            }
            Head::Lying(id) => self.take_lying(id),
        }
        let note = format!(
            "bound: {}; it holds {:.0} N",
            describe(&Item::Made(Box::new(made.clone()))),
            made.hold
        );
        self.swap_held(by, Item::Made(Box::new(made)))?;
        self.note(by, note);
        Ok(())
    }

    /// A strip to bind with: the first in avatar `by`'s pockets (its pocket and place), or else
    /// one lying within reach (its id).
    fn find_strip(&self, by: EntityId) -> Option<(Piece, StripAt)> {
        if let Some(pockets) = self.items.pockets.get(&by) {
            for (p, s) in pockets.iter().enumerate() {
                for (k, i) in s.items.iter().enumerate() {
                    if let Item::Piece(piece, _) = i
                        && is_strip(piece)
                    {
                        return Some((piece.clone(), StripAt::Pocket(p, k)));
                    }
                }
            }
        }
        let ids: Vec<u64> = self.items.laid.keys().copied().collect();
        ids.into_iter()
            .find_map(|id| match self.lying_in_reach(by, id) {
                Ok((Item::Piece(piece, _), ..)) if is_strip(&piece) => {
                    Some((piece, StripAt::Lying(id)))
                }
                _ => None,
            })
    }

    /// A strip in avatar `by`'s pockets (its pocket and place): to lash with (G2.3).
    pub(super) fn find_pocket_strip(&self, by: EntityId) -> Option<(usize, usize)> {
        match self.find_strip(by)? {
            (_, StripAt::Pocket(p, k)) => Some((p, k)),
            (_, StripAt::Lying(_)) => None,
        }
    }

    /// What avatar `by` holds, made into `item`: the old put down, the new taken in the hands.
    fn swap_held(&mut self, by: EntityId, item: Item) -> Result<(), String> {
        let held = self.carried(by).ok_or("nothing in hand")?;
        self.let_go(by);
        self.remove_item(held);
        self.put_in_hands(by, item)
    }
}

/// Where a strip to bind with is: in a pocket (its pocket and place), or lying (its id).
enum StripAt {
    Pocket(usize, usize),
    Lying(u64),
}

/// Whether `piece` is a strip: a block ten times wider than it is thin.
fn is_strip(piece: &Piece) -> bool {
    matches!(piece.form, Form::Block { size } if size.min_element() * 10.0 < size.max_element())
}
