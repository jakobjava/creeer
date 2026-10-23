//! What lies loose about the player, as this client makes it (G1.6c, ADR-0029): the same scatter
//! as the server's, cell by cell near the camera, less what the server says was taken. Drawn as
//! plain boxes in their rock's colour (or the wood's), and aimed at as the crosshair's ray meets
//! them; nothing of it is simulated here.

use creeer_math::glam::{DQuat, DVec3};
use creeer_net::message::Laid;
use creeer_sim::scatter::{Cell, Loose, Lying, Scatter};
use creeer_terrain::Terrain;
use std::collections::{BTreeMap, BTreeSet};

/// How far from the camera loose things are made and drawn (m).
const DRAWN: f64 = 40.0;
/// The colour branches are drawn in: the palette's `timber_weathered` (dead wood).
const DEADWOOD: [u8; 3] = [0x5e, 0x4f, 0x3f];

/// The loose things near the camera.
#[derive(Debug)]
pub struct LooseView {
    scatter: Scatter,
    /// The branches' woods' densities (kg/m³), by the scatter's index.
    woods: Vec<f64>,
    cells: BTreeMap<Cell, Vec<Loose>>,
    /// The terrain's edits the cells were made with (dug ground moves what lies on it).
    edits: (u64, usize),
}

impl LooseView {
    /// For `terrain`, from the packs; none if they have no scatter rules.
    pub fn new(terrain: &Terrain) -> Option<Self> {
        let scatter = Scatter::from_data(&crate::content::data(), terrain).ok()?;
        let materials = creeer_materials::MaterialTable::from_data(&crate::content::data()).ok()?;
        let woods = (scatter.woods.iter())
            .map(|w| materials.get(w).map_or(0.0, |m| m.density))
            .collect();
        Some(Self {
            scatter,
            woods,
            cells: BTreeMap::new(),
            edits: (0, 0),
        })
    }

    /// Keeps the cells within reach of the camera at `eye`, made anew where the edits changed.
    pub fn update(&mut self, terrain: &Terrain, eye: DVec3, edits: (u64, usize)) {
        if edits != self.edits {
            self.cells.clear();
            self.edits = edits;
        }
        let near = self.scatter.cells_near(eye, DRAWN);
        self.cells.retain(|c, _| near.contains(c));
        for cell in near {
            self.cells
                .entry(cell)
                .or_insert_with(|| self.scatter.in_cell(terrain, cell));
        }
    }

    /// Everything lying about: the scatter's, less what was `taken`, and the items the server
    /// laid at rest (`laid`, G1.6). `rocks` gives a stone's colour.
    pub fn things(
        &self,
        taken: &BTreeSet<u64>,
        laid: &BTreeMap<u64, Laid>,
        rocks: &creeer_terrain::geology::rocks::RockTable,
    ) -> Vec<Thing> {
        let scattered = (self.cells.values().flatten())
            .filter(|l| !taken.contains(&l.id))
            .map(|l| match l.what {
                Lying::Stone { rock, size } => Thing {
                    id: l.id,
                    centre: l.centre,
                    turn: l.turn,
                    half: size * 0.5,
                    colour: (rocks.rocks.get(usize::from(rock))).map_or([128; 3], |r| r.color),
                    word: "stone",
                    mass: (rocks.rocks.get(usize::from(rock))).map_or(0.0, |r| r.density)
                        * std::f64::consts::PI
                        / 6.0
                        * size.x
                        * size.y
                        * size.z,
                },
                Lying::Branch {
                    wood,
                    length,
                    radius,
                } => Thing {
                    id: l.id,
                    centre: l.centre,
                    turn: l.turn,
                    half: DVec3::new(length * 0.5, radius, radius),
                    colour: DEADWOOD,
                    word: "branch",
                    mass: self.woods.get(wood).copied().unwrap_or(0.0)
                        * std::f64::consts::PI
                        * radius
                        * radius
                        * length,
                },
            });
        let laid = laid.values().map(|l| Thing {
            id: l.id,
            centre: l.centre,
            turn: l.turn,
            half: l.half,
            colour: l.colour,
            mass: l.mass,
            word: match l.word.as_str() {
                "stone" => "stone",
                "flake" => "flake",
                "worked stone" => "worked stone",
                "stick" => "stick",
                "strip" => "strip",
                "tool" => "tool",
                "grit" => "grit",
                "chips" => "chips",
                _ => "thing",
            },
        });
        scattered.chain(laid).collect()
    }
}

/// Something lying on the ground, as drawn and aimed at: a box about an ellipsoid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thing {
    pub id: u64,
    pub centre: DVec3,
    pub turn: DQuat,
    pub half: DVec3,
    pub colour: [u8; 3],
    pub word: &'static str,
    /// Its mass (kg: what lifting it tells).
    pub mass: f64,
}

impl Thing {
    /// A word for it and its size (never its rock: telling flint from granite is the player's
    /// to learn, `docs/g1-plan.md` principle 9).
    pub fn name(&self) -> String {
        format!("{}, {:.0} cm", self.word, self.half.max_element() * 200.0)
    }
}

/// The first of `things` the ray from `origin` along unit `direction` meets within `length` (m),
/// and how far along: each as the ellipsoid in its box.
pub fn aim(things: &[Thing], origin: DVec3, direction: DVec3, length: f64) -> Option<(Thing, f64)> {
    let mut best: Option<(Thing, f64)> = None;
    for l in things {
        // In the ellipsoid's own frame, scaled to a unit ball.
        let back = l.turn.conjugate();
        let o = (back * (origin - l.centre)) / l.half;
        let d = (back * direction) / l.half;
        let (a, b, c) = (d.dot(d), o.dot(d), o.dot(o) - 1.0);
        let disc = b * b - a * c;
        if disc < 0.0 {
            continue;
        }
        let t = (-b - disc.sqrt()) / a;
        if t >= 0.0 && t <= length && best.is_none_or(|(_, s)| t < s) {
            best = Some((*l, t));
        }
    }
    best
}

impl LooseView {
    /// Everything lying about into `out`: a made thing as its pieces (G1.6), else as its box, in
    /// its colours.
    pub fn draw(
        &self,
        out: &mut crate::physics_sandbox::PhysicsSnapshot,
        c: &creeer_server::Connection,
        rocks: &creeer_terrain::geology::rocks::RockTable,
    ) {
        use crate::physics_sandbox::Look;
        for t in self.things(&c.taken, &c.laid, rocks) {
            let parts = (c.laid.get(&t.id))
                .map(|l| l.parts.as_slice())
                .unwrap_or_default();
            let boxes: Vec<(DVec3, DQuat, DVec3, [u8; 3])> = if parts.is_empty() {
                vec![(t.centre, t.turn, t.half, t.colour)]
            } else {
                (parts.iter())
                    .map(|p| (t.centre + t.turn * p.at, t.turn * p.turn, p.half, p.colour))
                    .collect()
            };
            for (centre, turn, half, colour) in boxes {
                out.boxes.push((centre, turn, half));
                out.box_look.push(Look::Colour(colour));
                out.box_age.push((0.0, 0.0));
            }
        }
    }
}
