//! A felled tree's parts (`docs/adr/0031-trees.md`, decision 6): a span of its stem with the
//! branches still on it, as a rigid body: its mass, its centre and inertia (its crown's mass
//! where it hangs), and its stem's hull. Branches are cut off a node thinnest first, so a part
//! only counts how many are gone at each node. And a standing tree's trunk as the boxes a
//! character walks into.

use super::grow::{Node, Tree, combine};
use creeer_math::det;
use creeer_math::glam::{DMat3, DQuat, DVec2, DVec3};
use creeer_physics::world::shape::StaticShape;

/// Corners of a ring of the stem's hull.
const RING: usize = 12;
/// The stem's hull has a ring at least this often (m).
const RINGS_EVERY: f64 = 3.0;
/// The thinnest a hull's ring is drawn to (m): a stem's top is a point.
const TIP: f64 = 0.01;
/// A standing trunk's boxes: each this tall (m), up to where the stem is this thin (m).
pub const BOX: f64 = 3.0;
const THINNEST_BOX: f64 = 0.03;
/// A square of a disc's area: its half side over the disc's radius (√π / 2).
const SQUARE: f64 = 0.886_226_925_452_758;

/// A part of a felled tree: the tree (to grow again), the span of its stem (m from its base), and
/// how many branches are cut off each node (thinnest first; none listed: none).
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub species: u16,
    pub seed: u64,
    pub dbh: f64,
    pub from: f64,
    pub to: f64,
    pub cut: Vec<u8>,
}

impl Part {
    /// How many branches are off node `k`.
    pub fn off(&self, k: usize) -> usize {
        usize::from(self.cut.get(k).copied().unwrap_or(0))
    }
}

impl Tree {
    /// Node `node`'s branches, thinnest first: the order they are cut off in.
    pub fn cut_order(node: &Node) -> Vec<usize> {
        let mut order: Vec<usize> = (0..node.branches.len()).collect();
        order.sort_by(|a, b| {
            node.branches[*a]
                .radius
                .total_cmp(&node.branches[*b].radius)
        });
        order
    }

    /// The branches still on `part`: (node, branch) indices.
    pub fn kept(&self, part: &Part) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (k, node) in self.nodes.iter().enumerate() {
            if node.z < part.from || node.z >= part.to {
                continue;
            }
            for &b in Self::cut_order(node).iter().skip(part.off(k)) {
                out.push((k, b));
            }
        }
        out
    }

    /// `part`'s mass (kg), its centre (tree frame) and inertia about it.
    pub fn part_mass(&self, part: &Part) -> (f64, DVec3, DMat3) {
        let mut lumps = vec![self.stem(part.from, part.to)];
        for (k, b) in self.kept(part) {
            let node = &self.nodes[k];
            lumps.push(self.branch(node, &node.branches[b]));
        }
        combine(&lumps)
    }

    /// The corners of `part`'s stem hull (tree frame): rings along it.
    pub fn part_hull(&self, part: &Part) -> Vec<DVec3> {
        let to = part.to.min(self.height);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few
        let n = ((to - part.from) / RINGS_EVERY).ceil().max(1.0) as usize;
        let mut points = Vec::with_capacity((n + 1) * RING);
        for k in 0..=n {
            #[allow(clippy::cast_precision_loss)]
            let z = part.from + (to - part.from) * k as f64 / n as f64;
            let r = self.radius(z).max(TIP);
            for c in 0..RING {
                #[allow(clippy::cast_precision_loss)]
                let (s, cs) = det::sin_cos(std::f64::consts::TAU * c as f64 / RING as f64);
                points.push(DVec3::new(r * cs, r * s, z));
            }
        }
        points
    }

    /// `part`'s hull with its base as the notch left it: rising from `pivot` (where it went
    /// over, in the cut's plane) towards `toward` (unit) at the notch's opening, so it turns on
    /// its hinge until the notch closes.
    pub fn notched_hull(&self, part: &Part, pivot: DVec2, toward: DVec2) -> Vec<DVec3> {
        let rise = det::tan(super::chop::OPENING);
        let mut points = self.part_hull(part);
        for p in points.iter_mut().filter(|p| p.z <= part.from) {
            p.z += (p.truncate() - pivot).dot(toward).max(0.0) * rise;
        }
        points
    }

    /// The boxes a standing trunk is to what walks into it, up to height `up_to` (m), the tree's
    /// base at `base` and its frame turned by `turn` (+z up the stem).
    pub fn trunk_boxes(&self, base: DVec3, turn: DQuat, up_to: f64) -> Vec<StaticShape> {
        let mut out = Vec::new();
        let mut z = 0.0;
        let top = up_to.min(self.height);
        while z < top {
            let h = BOX.min(top - z);
            let r = self.radius(z);
            if r < THINNEST_BOX {
                break;
            }
            out.push(StaticShape::Box {
                center: base + turn * DVec3::new(0.0, 0.0, z + 0.5 * h),
                rotation: turn,
                // A square of the disc's area.
                half: DVec3::new(r * SQUARE, r * SQUARE, 0.5 * h),
            });
            z += h;
        }
        out
    }
}
