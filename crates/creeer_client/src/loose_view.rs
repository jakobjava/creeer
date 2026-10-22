//! What lies loose about the player, as this client makes it (G1.6c, ADR-0029): the same scatter
//! as the server's, cell by cell near the camera, less what the server says was taken. Drawn as
//! plain boxes in their rock's colour (or the wood's), and aimed at as the crosshair's ray meets
//! them; nothing of it is simulated here.

use creeer_math::glam::{DQuat, DVec3};
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
    cells: BTreeMap<Cell, Vec<Loose>>,
    /// The terrain's edits the cells were made with (dug ground moves what lies on it).
    edits: (u64, usize),
}

impl LooseView {
    /// For `terrain`, from the packs; none if they have no scatter rules.
    pub fn new(terrain: &Terrain) -> Option<Self> {
        let scatter = Scatter::from_data(&crate::content::data(), terrain).ok()?;
        Some(Self {
            scatter,
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

    /// Everything drawn, less what was `taken`.
    pub fn lying<'a>(&'a self, taken: &'a BTreeSet<u64>) -> impl Iterator<Item = &'a Loose> {
        (self.cells.values().flatten()).filter(|l| !taken.contains(&l.id))
    }

    /// Each as a box: centre, frame, half extents and colour (`rocks` gives a stone's).
    pub fn boxes(
        &self,
        taken: &BTreeSet<u64>,
        rocks: &creeer_terrain::geology::rocks::RockTable,
    ) -> Vec<(DVec3, DQuat, DVec3, [u8; 3])> {
        (self.lying(taken))
            .map(|l| match l.what {
                Lying::Stone { rock, size } => {
                    let colour = rocks
                        .rocks
                        .get(usize::from(rock))
                        .map_or([128; 3], |r| r.color);
                    (l.centre, l.turn, size * 0.5, colour)
                }
                Lying::Branch { length, radius, .. } => {
                    let half = DVec3::new(length * 0.5, radius, radius);
                    (l.centre, l.turn, half, DEADWOOD)
                }
            })
            .collect()
    }

    /// The first loose thing the ray from `origin` along unit `direction` meets within
    /// `length` (m), and how far along: each as the ellipsoid in its box.
    pub fn aim(
        &self,
        taken: &BTreeSet<u64>,
        origin: DVec3,
        direction: DVec3,
        length: f64,
    ) -> Option<(Loose, f64)> {
        let mut best: Option<(Loose, f64)> = None;
        for l in self.lying(taken) {
            let half = match l.what {
                Lying::Stone { size, .. } => size * 0.5,
                Lying::Branch { length, radius, .. } => DVec3::new(length * 0.5, radius, radius),
            };
            // In the ellipsoid's own frame, scaled to a unit ball.
            let back = l.turn.conjugate();
            let o = (back * (origin - l.centre)) / half;
            let d = (back * direction) / half;
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
}

/// A word for what lies loose: a stone and its size (never its rock: telling flint from granite
/// is the player's to learn, `docs/g1-plan.md` principle 9), or a branch.
pub fn name(loose: &Loose) -> String {
    match loose.what {
        Lying::Stone { size, .. } => format!("stone, {:.0} cm", size.x * 100.0),
        Lying::Branch { length, .. } => format!("branch, {:.1} m", length),
    }
}
