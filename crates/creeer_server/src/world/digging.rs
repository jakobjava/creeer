//! Digging (G1.5, ADR-0032): a blow on the ground breaks out what its energy pays for
//! (`creeer_sim::dig`), handed out at once as a heap of loose matter beside the spot, of the
//! ground's composition there (an ore's at its deposit's grade); the head that struck wears.
//! Every blow cuts the terrain at once (G2.9): each spot has one hole that grows, a dent, then
//! a shaft a post stands in, then wider, sized so the ground loses exactly what was dug there,
//! to a thousandth (`creeer_terrain::dug`), its brush grown in its place in the edit log.
//! Mass is kept: what the heaps hold is the ground's density times what was cut plus what the
//! spots still owe (a save's, and what a hole grown as wide as it grows cannot take).

use super::*;
use creeer_math::glam::DQuat;
use creeer_sim::dig;
use creeer_sim::items::{Bulk, Item, Piece};
use creeer_sim::strike::Swing;
use creeer_terrain::geology::rocks::Strength;
use std::collections::BTreeMap;

/// A ledger's cell (m): blows within it dig the same spot (unless they fall in a hole).
const SPOT: f64 = 0.5;
/// A blow within this of a spot's hole digs that hole (m): its floor and walls.
const IN_HOLE: f64 = 0.1;
/// A hand-dug hole's radius (m): a spade's width, a post's hole (25–30 cm across).
pub(super) const HOLE: f64 = 0.15;
/// How deep a hole is dug from above by hand before it must widen (m): an arm and a tool's
/// reach below the ground one kneels or stands on.
pub(super) const REACH: f64 = 1.0;
/// The widest a spot's hole grows (m); beyond, blows on its walls and floor dig spots of their
/// own.
pub(super) const WIDEST: f64 = 0.6;
/// How far from the spot the heap is laid, toward the digger (m): beyond the widest hole.
const HEAP: f64 = WIDEST + 0.3;
/// Broken rock and dug soil take more room than they did in the ground: swell factors about
/// 1.5 and 1.25 (Caterpillar Performance Handbook's tables).
const SWELL_ROCK: f64 = 1.5;
const SWELL_SOIL: f64 = 1.25;
/// Rock and soil's specific heat (J/(kg·K)), and the ground's temperature where it is dug (K).
const SPECIFIC_HEAT: f64 = 800.0;
const GROUND: f64 = 288.15;

/// What is dug, by spot.
#[derive(Clone, Debug, Default)]
pub(super) struct Digging {
    pub(super) pits: BTreeMap<[i32; 3], Pit>,
    /// What was cut from the terrain since the server started (m³).
    pub(super) cut: f64,
}

/// A spot: where it was first struck, what was dug there for its brush (m³) and what its brush
/// takes (m³: the same, to a thousandth, unless it is as large as it grows), its brush (the
/// edit's index and its shape's parameter) and the heap its matter lies in (laid).
#[derive(Clone, Debug, Default)]
pub(super) struct Pit {
    pub(super) centre: DVec3,
    pub(super) volume: f64,
    pub(super) took: f64,
    pub(super) brush: Option<(usize, f64)>,
    pub(super) heap: Option<u64>,
}

impl Pit {
    /// A spot first struck at `centre`, owed `volume` m³ (dug and not yet cut: a save's).
    pub(super) fn new(centre: DVec3, volume: f64) -> Self {
        Self {
            centre,
            volume,
            ..Self::default()
        }
    }

    /// What was dug there and its brush does not take (m³).
    pub(super) fn owed(&self) -> f64 {
        self.volume - self.took
    }
}

pub(super) fn spot(p: DVec3) -> [i32; 3] {
    #[allow(clippy::cast_possible_truncation)] // planet-sized coordinates fit i32 cells
    let cell = (p / SPOT).floor().as_ivec3().to_array();
    cell
}

impl ServerWorld {
    /// Avatar `by`'s blow of `swing` (none: bare hands) on the ground at `at`, with the
    /// catalogue's tool `tool` or what the hands hold.
    pub(super) fn dig(&mut self, by: EntityId, at: DVec3, (swing, tool): (&Swing, Option<u8>)) {
        let (_, gradient) = self.terrain.density_gradient(at);
        let up = gradient.try_normalize().unwrap_or_else(|| at.normalize());
        let inside = at - up * 0.02;
        let Some(rock) = self.terrain.rock_at(inside) else {
            return;
        };
        let rocks = &self.terrain.planet().geology.rocks;
        let r = &rocks.rocks[usize::from(rock)];
        let g = (self.world.planet.as_ref()).map_or(9.81, |p| p.acceleration(at).length());
        let excavation = dig::excavation_energy(r.strength, r.density, g);
        let solid = matches!(r.strength, Strength::Solid { .. });
        let ground = dig::mean_hardness(rocks.hardnesses(rock));
        // The ground does not give: the blow's energy is its effective mass's.
        let energy = 0.5 * swing.mass * swing.speed * swing.speed;
        let Some((hardness, tip)) = self.digging_head(by, tool) else {
            if solid {
                self.note(by, "too hard for bare hands".into());
                return;
            }
            let volume = dig::dug(
                energy,
                dig::efficiency(dig::FINGERS, 1.0, ground, false),
                excavation,
            );
            self.dug(by, (at, inside), rock, volume);
            return;
        };
        let volume = dig::dug(
            energy,
            dig::efficiency(tip, hardness, ground, solid),
            excavation,
        );
        let lost = dig::wear(energy, hardness, rocks.hardnesses(rock), excavation);
        self.wear_head(by, tool, lost);
        // The head's binding takes the blow's force over the depth it went in (soil gives: a
        // blow into it is no blow on stone).
        let impulse = 2.0 * energy / swing.speed.max(1e-9);
        let force = dig::peak_force(energy, volume, (impulse, super::making::CONTACT));
        self.binding_bears(by, force);
        self.dug(by, (at, inside), rock, volume);
    }

    /// The head that digs for avatar `by`: its hardness (Pa) and its tip's radius (m); none if
    /// the hands are bare.
    fn digging_head(&self, by: EntityId, tool: Option<u8>) -> Option<(f64, f64)> {
        if let Some(t) = tool {
            let t = self.tools.get(usize::from(t))?;
            return Some((t.hardness, t.tip));
        }
        let head = self.items.bodies.get(&self.carried(by)?)?.head()?;
        let hardness = self.items.densities.as_ref()?.hardness(&head.material)?;
        Some((hardness, tip(head)))
    }

    /// The head of what avatar `by` holds loses `lost` m³ (the catalogue's tools are not worn).
    fn wear_head(&mut self, by: EntityId, tool: Option<u8>, lost: f64) {
        if tool.is_some() || lost <= 0.0 {
            return;
        }
        let Some(held) = self.carried(by) else { return };
        let Some(item) = self.items.bodies.get_mut(&held) else {
            return;
        };
        let piece = match item {
            Item::Made(a) => {
                let k = a.swinging().head;
                a.parts.get_mut(k).map(|p| &mut p.piece)
            }
            Item::Piece(p, _) => Some(p),
            Item::Bulk(_) => None,
        };
        if let Some(piece) = piece {
            worn(piece, lost);
        }
    }

    /// `volume` m³ of rock `rock` dug by `by` at `at` (`inside` the ground there): laid on its
    /// spot's heap, and cut from the terrain at once by its spot's hole, grown.
    fn dug(&mut self, by: EntityId, (at, inside): (DVec3, DVec3), rock: u16, volume: f64) {
        if volume.is_nan() || volume <= 0.0 {
            self.note(by, "it barely marked it".into());
            return;
        }
        let geology = &self.terrain.planet().geology;
        let r = &geology.rocks.rocks[usize::from(rock)];
        // A blow's volume is centimetres across, a deposit's grade varies over metres: its
        // centre's grade is its own.
        let grade = (geology
            .sample_at(inside, self.terrain.surface_height(inside))
            .deposit)
            .filter(|hit| geology.deposits[usize::from(hit.kind)].rock == rock)
            .map_or(1.0, |hit| hit.grade);
        let swell = match r.strength {
            Strength::Solid { .. } => SWELL_ROCK,
            Strength::Loose { .. } => SWELL_SOIL,
        };
        let bulk = Bulk {
            mass: r.density * volume,
            composition: geology.rocks.composition(rock, grade),
            temperature: GROUND,
            specific_heat: SPECIFIC_HEAT,
            grain: 0.5 * creeer_math::det::cbrt(volume),
            bulk_density: r.density / swell,
        };
        let key = self.spot_of(at);
        let pit = (self.digging.pits.entry(key)).or_insert_with(|| Pit::new(at, 0.0));
        pit.volume += volume;
        let heap = pit.heap;
        let mass = bulk.mass;
        let heap = self.heap_on(heap, bulk, (at, by));
        if let Some(pit) = self.digging.pits.get_mut(&key) {
            pit.heap = Some(heap);
        }
        self.note(by, format!("dug {:.2} kg ({:.2} l)", mass, volume * 1e3));
        self.cut_pit(key);
    }

    /// The spot a blow at `at` digs: the hole it falls in, or its cell's.
    fn spot_of(&self, at: DVec3) -> [i32; 3] {
        let edits = self.terrain.edits();
        (self.digging.pits.iter())
            .find(|(_, p)| {
                p.brush.is_some_and(|(k, _)| {
                    (edits.log().get(k)).is_some_and(|e| e.brush().distance(at) < IN_HOLE)
                })
            })
            .map_or_else(|| spot(at), |(key, _)| *key)
    }

    /// `bulk` added to heap `heap` if it still lies, or laid as a new heap beside `at` toward
    /// avatar `by`: the heap's id.
    fn heap_on(&mut self, heap: Option<u64>, bulk: Bulk, (at, by): (DVec3, EntityId)) -> u64 {
        if let Some(id) = heap
            && let Some((Item::Bulk(lying), ..)) = self.items.laid.get_mut(&id)
        {
            lying.merge(&bulk);
            self.items.laid_log.push(id);
            return id;
        }
        let up = character::plumb(&self.world, at);
        let from = (self.index(by)).map_or(at, |i| self.world.bodies[i].structure.position[0]);
        let toward = (from - at).reject_from(up).normalize_or(DVec3::ZERO);
        let turn = DQuat::from_rotation_arc(DVec3::Z, up);
        self.lay(Item::Bulk(bulk), at + toward * HEAP, turn)
    }
}

/// The radius of what strikes on `head` (m): its edge's, or half its thinnest size (a face).
fn tip(head: &Piece) -> f64 {
    head.edge
        .map_or_else(|| 0.5 * head.form.extents()[2], |e| e.radius)
}

/// `piece` worn by `lost` m³: its wear (the share of it lost) grows, and its edge blunts.
fn worn(piece: &mut Piece, lost: f64) {
    let volume = piece.form.volume();
    if volume <= 0.0 {
        return;
    }
    piece.wear = (piece.wear + lost / volume).min(1.0);
    if let Some(edge) = piece.edge.as_mut() {
        let r = dig::worn_tip(edge.angle, edge.length, piece.wear * volume);
        edge.radius = edge.radius.max(r);
    }
}

#[cfg(test)]
#[path = "digging_tests.rs"]
mod tests;
