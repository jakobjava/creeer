//! A skinned model's levels of detail as the game draws them (G1.3e, ADR-0026): its vertices
//! grouped level by level, so a character skins only the level it is drawn at, and each level's
//! error against the close-up one, by which the level is chosen (half a pixel, as the
//! primitives' levels, decision 11).

use crate::model::Model;
use crate::model_skin::LEVELS;
use creeer_math::glam::Vec3;
use std::collections::HashMap;
use std::ops::Range;

/// `model` with its vertices reordered so each level's are together, in level order (a part's
/// index runs keep their places; the indices are renumbered), and each level's vertices.
///
/// # Panics
/// If a vertex is used by parts of two levels (a model the reader would not make).
pub fn grouped(model: &Model) -> (Model, [Range<u32>; LEVELS as usize]) {
    let mut level_of = vec![None; model.vertices.len()];
    for part in &model.parts {
        for &i in &model.indices[part.first..part.first + part.count] {
            let at = &mut level_of[i as usize];
            assert!(at.is_none_or(|l| l == part.level), "a vertex in two levels");
            *at = Some(part.level);
        }
    }
    let mut order: Vec<usize> = (0..model.vertices.len()).collect();
    // Stable: within a level the file's order stays. Vertices no part uses go last.
    order.sort_by_key(|&v| level_of[v].unwrap_or(u8::MAX));
    let mut new_index = vec![0u32; model.vertices.len()];
    for (new, &old) in order.iter().enumerate() {
        new_index[old] = u32::try_from(new).expect("fewer than 2³² vertices");
    }
    let mut ranges: [Range<u32>; LEVELS as usize] = std::array::from_fn(|_| 0..0);
    for (level, range) in ranges.iter_mut().enumerate() {
        let level = u8::try_from(level).expect("few levels");
        let first = order.iter().position(|&v| level_of[v] == Some(level));
        let count = order
            .iter()
            .filter(|&&v| level_of[v] == Some(level))
            .count();
        let first = u32::try_from(first.unwrap_or(0)).expect("fits");
        *range = first..first + u32::try_from(count).expect("fits");
    }
    let out = Model {
        vertices: order.iter().map(|&v| model.vertices[v]).collect(),
        indices: model
            .indices
            .iter()
            .map(|&i| new_index[i as usize])
            .collect(),
        bones: model.bones.clone(),
        parts: model.parts.clone(),
    };
    (out, ranges)
}

/// Each level's error against the close-up level (m): over every piece, the largest distance
/// from a point of either to the other's surface (the Hausdorff distance, taken at the
/// vertices both ways). The close-up level's is 0.
pub fn level_errors(model: &Model) -> [f32; LEVELS as usize] {
    let mut out = [0.0f32; LEVELS as usize];
    let pieces: Vec<&str> = {
        let mut p: Vec<&str> = model.parts.iter().map(|p| p.piece.as_str()).collect();
        p.sort_unstable();
        p.dedup();
        p
    };
    for piece in pieces {
        let triangles = |level: u8| -> Vec<[Vec3; 3]> {
            (model.parts.iter())
                .filter(|p| p.piece == piece && p.level == level)
                .flat_map(|p| model.indices[p.first..p.first + p.count].chunks(3))
                .map(|t| {
                    t.iter()
                        .map(|&i| Vec3::from(model.vertices[i as usize].position))
                })
                .map(|mut t| [t.next(), t.next(), t.next()].map(Option::unwrap_or_default))
                .collect()
        };
        let close = triangles(0);
        if close.is_empty() {
            continue;
        }
        let close_grid = Grid::new(&close);
        for level in 1..LEVELS {
            let coarse = triangles(level);
            if coarse.is_empty() {
                continue;
            }
            let coarse_grid = Grid::new(&coarse);
            let one_way = |from: &[[Vec3; 3]], to: &Grid| {
                (from.iter().flatten())
                    .map(|&p| to.distance(p))
                    .fold(0.0f32, f32::max)
            };
            let error = one_way(&coarse, &close_grid).max(one_way(&close, &coarse_grid));
            let at = &mut out[usize::from(level)];
            *at = at.max(error);
        }
    }
    // A coarser level is never better than a finer one.
    for level in 1..LEVELS as usize {
        out[level] = out[level].max(out[level - 1]);
    }
    out
}

/// Triangles in a uniform grid, for the nearest one to a point.
struct Grid<'a> {
    triangles: &'a [[Vec3; 3]],
    cell: f32,
    cells: HashMap<[i32; 3], Vec<u32>>,
}

impl<'a> Grid<'a> {
    /// A grid of cells about the triangles' mean size.
    fn new(triangles: &'a [[Vec3; 3]]) -> Self {
        #[allow(clippy::cast_precision_loss)] // a mean over a model's triangles
        let mean = triangles
            .iter()
            .map(|t| (t[1] - t[0]).length().max((t[2] - t[0]).length()))
            .sum::<f32>()
            / triangles.len() as f32;
        let cell = mean.max(1e-3);
        let mut cells: HashMap<[i32; 3], Vec<u32>> = HashMap::new();
        for (k, t) in triangles.iter().enumerate() {
            let lo = t[0].min(t[1]).min(t[2]);
            let hi = t[0].max(t[1]).max(t[2]);
            let (a, b) = (key(lo, cell), key(hi, cell));
            for x in a[0]..=b[0] {
                for y in a[1]..=b[1] {
                    for z in a[2]..=b[2] {
                        let k = u32::try_from(k).expect("fewer than 2³² triangles");
                        cells.entry([x, y, z]).or_default().push(k);
                    }
                }
            }
        }
        Self {
            triangles,
            cell,
            cells,
        }
    }

    /// The distance from `p` to the nearest triangle: shells of cells searched outwards
    /// until no farther shell can hold a nearer one.
    fn distance(&self, p: Vec3) -> f32 {
        let centre = key(p, self.cell);
        let mut best = f32::INFINITY;
        for ring in 0..RINGS {
            #[allow(clippy::cast_precision_loss)] // a few rings
            if best <= (ring as f32 - 1.0).max(0.0) * self.cell {
                break;
            }
            for x in -ring..=ring {
                for y in -ring..=ring {
                    for z in -ring..=ring {
                        if x.abs().max(y.abs()).max(z.abs()) != ring {
                            continue;
                        }
                        let at = [centre[0] + x, centre[1] + y, centre[2] + z];
                        for &t in self.cells.get(&at).into_iter().flatten() {
                            let t = &self.triangles[t as usize];
                            best = best.min((closest(p, t) - p).length());
                        }
                    }
                }
            }
        }
        #[allow(clippy::cast_precision_loss)] // a few rings
        if best > (RINGS - 1) as f32 * self.cell {
            // Farther than the shells searched: each triangle, one by one.
            for t in self.triangles {
                best = best.min((closest(p, t) - p).length());
            }
        }
        best
    }
}

/// Shells of cells searched before every triangle is.
const RINGS: i32 = 24;

fn key(p: Vec3, cell: f32) -> [i32; 3] {
    #[allow(clippy::cast_possible_truncation)] // models are metres across
    (p / cell).floor().as_ivec3().to_array()
}

/// The point of triangle `t` nearest `p` (Ericson, *Real-Time Collision Detection*, 5.1.5).
fn closest(p: Vec3, [a, b, c]: &[Vec3; 3]) -> Vec3 {
    let (ab, ac, ap) = (*b - *a, *c - *a, p - *a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return *a;
    }
    let bp = p - *b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return *b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return *a + ab * (d1 / (d1 - d3));
    }
    let cp = p - *c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return *c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return *a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return *b + (*c - *b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    *a + ab * (vb * denom) + ac * (vc * denom)
}

#[cfg(test)]
#[path = "model_levels_tests.rs"]
mod tests;
