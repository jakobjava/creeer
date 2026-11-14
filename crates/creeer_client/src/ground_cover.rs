//! Decoratives near the camera (G2.11, ADR-0037): grass tufts and grit on the drawn ground,
//! from the packs' `ground/decoratives.ron`.
//!
//! The ground is cut into cells about [`CELL`] metres a side on the cube-sphere's faces, fixed to
//! the planet; what stands in a cell comes from the planet's seed and the cell alone, so it is
//! the same every time it is seen and does not change as the camera moves or the levels of
//! detail switch. A worker thread fills the cells within [`RADIUS`] of the camera; each frame the
//! main thread hands the renderer what stands in them, camera-relative, shrinking into the ground
//! towards the edge (no popping), and pressed flat where something lies on it, so a stick or a
//! stone in the grass stays in sight. A tuft stands only where the shader draws its layer
//! (`creeer_render::ground_pattern`), and nothing stands on dug ground.

use crate::ground_look::GroundLook;
use creeer_art::ground::{Decorative, Ground, Shape};
use creeer_math::glam::{DVec3, Vec3};
use creeer_render::cover::CoverInstance;
use creeer_terrain::Terrain;
use creeer_terrain::cube::CubeCoord;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

/// A cell's side (m).
pub const CELL: f64 = 4.0;
/// How far from the camera decoratives stand (m).
pub const RADIUS: f64 = 24.0;
/// Nothing stands on ground steeper than this (rad, about 50°).
const STEEPEST: f64 = 0.87;
/// Tufts within this of a thing lying on the ground are pressed flat (m, beyond its size).
const PRESSED: f64 = 0.12;

/// A cell: its face and place on the face's grid.
type Key = (u8, i32, i32);

/// One decorative in a cell.
#[derive(Clone, Copy, Debug)]
struct Placed {
    /// From the cell's origin (m).
    offset: Vec3,
    instance: CoverInstance,
    shape: Shape,
}

/// What stands in a cell.
#[derive(Debug, Default)]
struct Cell {
    origin: DVec3,
    placed: Vec<Placed>,
}

/// The decoratives about the camera.
pub struct GroundCover {
    cells: HashMap<Key, Cell>,
    asked: HashSet<Key>,
    ask: Sender<Key>,
    done: Receiver<(Key, Cell)>,
    /// Cell side in face units.
    step: f64,
    /// Where the wanted cells were last worked out, and they.
    wanted: (DVec3, Vec<Key>),
}

impl GroundCover {
    /// Decoratives on `terrain` as the client's packs describe them (none if they do not load).
    pub fn new(terrain: &Arc<Terrain>) -> Self {
        let data = crate::content::data();
        let decoratives = Ground::from_data(&data).map(|g| g.decoratives);
        let palette = creeer_art::palette::Palette::from_data(&data).unwrap_or_default();
        let decoratives = decoratives.unwrap_or_default();
        let colours = (decoratives.iter())
            .filter_map(|d| d.token.clone())
            .filter_map(|t| Some((t.clone(), palette.token(&t)?.shade.fixed()?.0)))
            .collect();
        let builder = Builder {
            terrain: Arc::clone(terrain),
            look: GroundLook::current(),
            decoratives,
            colours,
            step: CELL / terrain.planet().radius,
        };
        let (ask, asked_rx) = channel::<Key>();
        let (done_tx, done) = channel();
        std::thread::Builder::new()
            .name("ground cover".into())
            .spawn(move || {
                for key in asked_rx {
                    if done_tx.send((key, builder.cell(key))).is_err() {
                        return;
                    }
                }
            })
            .expect("a thread for the ground cover");
        Self {
            cells: HashMap::new(),
            asked: HashSet::new(),
            ask,
            done,
            step: CELL / terrain.planet().radius,
            wanted: (DVec3::splat(f64::NAN), Vec::new()),
        }
    }

    /// Forgets the cells an edit reaches (from `lo` to `hi`), to be filled again.
    pub fn invalidate(&mut self, lo: DVec3, hi: DVec3) {
        let middle = (lo + hi) * 0.5;
        let reach = (hi - lo).length() * 0.5 + CELL * 1.5;
        self.cells
            .retain(|_, c| (c.origin - middle).length() > reach);
        self.wanted.0 = DVec3::splat(f64::NAN);
    }

    /// This frame's decoratives about `camera` (tufts, grit), camera-relative, `forward` the
    /// way it looks; `things` lying about (centre, radius) press tufts flat.
    pub fn frame(
        &mut self,
        camera: DVec3,
        forward: DVec3,
        things: &[(DVec3, f64)],
    ) -> (Vec<CoverInstance>, Vec<CoverInstance>) {
        for (key, cell) in self.done.try_iter() {
            self.asked.remove(&key);
            self.cells.insert(key, cell);
        }
        if (camera - self.wanted.0).length() > 1.0 || self.wanted.0.is_nan() {
            self.wanted = (camera, self.cells_near(camera));
            let keep: HashSet<Key> = self.wanted.1.iter().copied().collect();
            self.cells.retain(|k, _| keep.contains(k));
        }
        for key in &self.wanted.1 {
            if !self.cells.contains_key(key) && self.asked.insert(*key) {
                let _ = self.ask.send(*key);
            }
        }
        let (mut tufts, mut grit) = (Vec::new(), Vec::new());
        for cell in self.cells.values() {
            let to = cell.origin - camera;
            // Behind the camera beyond the cell's own size: not seen.
            if to.dot(forward) < -CELL * 1.5 {
                continue;
            }
            let near: Vec<&(DVec3, f64)> = (things.iter())
                .filter(|(c, r)| (*c - cell.origin).length() < r + CELL * 1.5)
                .collect();
            for p in &cell.placed {
                let at = cell.origin + p.offset.as_dvec3();
                let distance = (at - camera).length();
                let mut i = p.instance;
                // Shrinks into the ground over the last quarter of its reach.
                let reach = (600.0 * f64::from(i.height.max(i.width))).min(RADIUS);
                let fade = ((reach - distance) / (0.25 * reach)).clamp(0.0, 1.0);
                #[allow(clippy::cast_possible_truncation)] // a share
                let fade = fade as f32;
                if fade <= 0.0 {
                    continue;
                }
                i.height *= fade;
                i.width *= fade;
                #[allow(clippy::cast_possible_truncation)] // camera-relative: near
                let rel = (at - camera).as_vec3();
                i.position = rel.to_array();
                if p.shape == Shape::Tuft {
                    if near.iter().any(|(c, r)| (at - *c).length() < r + PRESSED) {
                        i.height *= 0.2;
                        i.lean = 1.0;
                    }
                    tufts.push(i);
                } else {
                    grit.push(i);
                }
            }
        }
        (tufts, grit)
    }

    /// The cells within [`RADIUS`] of `p`: the disc about it sampled at a third of a cell.
    fn cells_near(&self, p: DVec3) -> Vec<Key> {
        let up = p.normalize();
        let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
        let spacing = CELL / 3.0;
        #[allow(clippy::cast_possible_truncation)] // a few dozen
        let n = (RADIUS / spacing).ceil() as i32;
        let mut out = HashSet::new();
        for a in -n..=n {
            for b in -n..=n {
                let off = (east * f64::from(a) + north * f64::from(b)) * spacing;
                if off.length() <= RADIUS + spacing {
                    out.insert(key_of(p + off, self.step));
                }
            }
        }
        out.into_iter().collect()
    }
}

impl crate::planet_view::PlanetView {
    /// Hands the renderer the decoratives about `camera`, pressed flat where things lie loose or
    /// bodies rest; returns how many cells are still being filled.
    pub fn update_cover(
        &mut self,
        renderer: &mut creeer_render::Renderer,
        camera: &creeer_render::Camera,
    ) -> usize {
        let Some(cover) = &mut self.cover else {
            return 0;
        };
        let c = &self.session.connection;
        let rocks = &self.terrain.planet().geology.rocks;
        let mut things: Vec<(DVec3, f64)> = (self.loose.as_ref())
            .map(|l| l.things(&c.taken, &c.laid, rocks))
            .unwrap_or_default()
            .iter()
            .filter(|t| (t.centre - camera.position).length() < RADIUS + 2.0)
            .map(|t| (t.centre, t.half.max_element()))
            .collect();
        things.extend(
            (c.poses().values())
                .map(|p| p.position)
                .filter(|p| (*p - camera.position).length() < RADIUS + 2.0)
                .map(|p| (p, 0.3)),
        );
        let (tufts, grit) = cover.frame(camera.position, camera.forward(), &things);
        renderer.set_cover(&tufts, &grit);
        cover.asked.len()
    }
}

/// The cell direction `d` lies in, for cells `step` face units a side.
fn key_of(d: DVec3, step: f64) -> Key {
    let c = CubeCoord::from_direction(d);
    #[allow(clippy::cast_possible_truncation)] // within the face: below 2^31
    let index = |s: f64| (s / step).floor() as i32;
    (c.face, index(c.s), index(c.t))
}

/// What fills a cell (on the worker thread).
struct Builder {
    terrain: Arc<Terrain>,
    look: Arc<GroundLook>,
    decoratives: Vec<Decorative>,
    /// The decoratives' tokens' colours (sRGB).
    colours: HashMap<String, [u8; 3]>,
    step: f64,
}

#[path = "ground_cover_cell.rs"]
mod cell;
#[cfg(test)]
#[path = "ground_cover_tests.rs"]
mod tests;
