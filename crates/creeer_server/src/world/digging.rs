//! Digging (G1.5, ADR-0032): a blow on the ground breaks out what its energy pays for
//! (`creeer_sim::dig`), handed out at once as a heap of loose matter beside the spot, of the
//! ground's composition there (an ore's at its deposit's grade); the head that struck wears.
//! Each spot keeps a ledger of what was dug there and not yet cut from the terrain; once it
//! holds the smallest pit the finest grid draws, the terrain is cut by a ball that takes out
//! of the field exactly what it holds, to a thousandth, the rest staying in the ledger
//! (`creeer_terrain::dug`). Mass is kept: what the heaps hold is the ground's density times
//! what was cut plus what the ledgers hold.

use super::*;
use creeer_math::glam::DQuat;
use creeer_sim::dig;
use creeer_sim::items::{Bulk, Item, Piece};
use creeer_sim::strike::Swing;
use creeer_terrain::dug::Field;
use creeer_terrain::geology::rocks::Strength;
use std::collections::BTreeMap;

/// A ledger's cell (m): blows within it dig the same spot.
const SPOT: f64 = 0.5;
/// The smallest and the largest pit cut (m): the finest grid draws a brush of half its cell.
const LEAST: f64 = 0.5;
const MOST: f64 = 0.6;
/// The field's sampling step for a pit (m), and how far about the pit it is sampled: the ball
/// and its round, as wide as itself.
const STEP: f64 = 0.1;
const FIELD: f64 = 2.0 * MOST;
/// Below this a ledger is not measured against the field: no pit takes less than a third of
/// its least half ball (m³).
const FIRST_LOOK: f64 = 2.0 / 9.0 * std::f64::consts::PI * LEAST * LEAST * LEAST;
/// How far from the spot the heap is laid, toward the digger (m): beyond the largest pit.
const HEAP: f64 = MOST + 0.3;
/// Broken rock and dug soil take more room than they did in the ground: swell factors about
/// 1.5 and 1.25 (Caterpillar Performance Handbook's tables).
const SWELL_ROCK: f64 = 1.5;
const SWELL_SOIL: f64 = 1.25;
/// Rock and soil's specific heat (J/(kg·K)), and the ground's temperature where it is dug (K).
const SPECIFIC_HEAT: f64 = 800.0;
const GROUND: f64 = 288.15;

/// What is dug and not yet cut from the terrain, by spot.
#[derive(Clone, Debug, Default)]
pub(super) struct Digging {
    pub(super) pits: BTreeMap<[i32; 3], Pit>,
    /// What was cut from the terrain since the server started (m³).
    pub(super) cut: f64,
}

/// A spot's ledger: where it was first struck, what was dug there and not yet cut (m³), the
/// heap its matter lies in (laid), and the field there as it was last measured (with the count
/// of edits it saw).
#[derive(Clone, Debug, Default)]
pub(super) struct Pit {
    pub(super) centre: DVec3,
    pub(super) volume: f64,
    heap: Option<u64>,
    field: Option<(usize, Field)>,
}

impl Pit {
    pub(super) fn new(centre: DVec3, volume: f64) -> Self {
        Self {
            centre,
            volume,
            ..Self::default()
        }
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
        // The head's binding takes the blow's whole impulse: the ground does not move.
        self.binding_bears(by, 2.0 * energy / swing.speed.max(1e-9));
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
    /// spot's heap, and the spot's ledger cut from the terrain when it holds a pit.
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
        let key = spot(at);
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

    /// Spot `key`'s ledger cut from the terrain if it holds a pit the finest grid draws.
    fn cut_pit(&mut self, key: [i32; 3]) {
        let (edits, centre) = (
            self.terrain.edits().len(),
            self.digging.pits.get(&key).map(|p| p.centre),
        );
        // The field measured before holds unless an edit since came near.
        let near = |seen: usize| {
            let log = self.terrain.edits();
            (log.log().get(seen..).unwrap_or_default().iter()).any(|e| {
                centre.is_some_and(|c| e.brush().distance(c) < FIELD + 2.0 * e.brush().radius())
            })
        };
        let stale = |pit: &Pit| pit.field.as_ref().is_none_or(|(seen, _)| near(*seen));
        let Some(pit) = self.digging.pits.get_mut(&key) else {
            return;
        };
        if pit.volume < FIRST_LOOK {
            return;
        }
        if stale(pit) {
            let field = Field::around(&self.terrain, pit.centre, FIELD, STEP);
            pit.field = Some((edits, field));
        }
        let (centre, volume) = (pit.centre, pit.volume);
        let Some((_, field)) = pit.field.as_ref() else {
            return;
        };
        let Some((radius, took)) = field.sphere_for(centre, volume, (LEAST, MOST)) else {
            return;
        };
        pit.volume -= took;
        self.digging.cut += took;
        pit.field = None;
        let (left, heap) = (pit.volume, pit.heap);
        self.terrain.edit(Edit::Dig(Brush::Sphere {
            center: centre,
            radius,
        }));
        self.world.wake_near(centre, radius * 2.0 + 1.0);
        // What is left over digs on in the pit's floor, a new spot.
        self.digging.pits.remove(&key);
        if left > 0.0 {
            let floor = centre - centre.normalize() * radius;
            let mut next = Pit::new(floor, left);
            next.heap = heap;
            let into = self.digging.pits.entry(spot(floor)).or_default();
            if into.volume > 0.0 {
                into.volume += left;
            } else {
                *into = next;
            }
        }
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
