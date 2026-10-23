//! Parts fastened together become one structure (G2.3a, ADR-0033): each part a member along
//! its axis, split into elements at its joints and every [`LONGEST`] metres, each fastening a
//! joint element between the two parts' axes. The structure starts as the parts moved: each
//! node with its part's rigid motion, so momentum is kept exactly.
//!
//! Each element collides as a prism round its axis (a twelve-sided one for a round part, a box
//! for a board), split in halves on its two end nodes, so the shape follows the member as it
//! bends.

use creeer_math::glam::DVec3;
use creeer_physics::structure::{JointLaw, Material, NodeId, Section, Structure, StructureBuilder};
use creeer_physics::world::hull::Hull;
use creeer_physics::world::shape::Shape;
use std::sync::Arc;

/// The longest element of a member (m): a member is split at least this often, so it bends
/// and buckles between its joints (buckling splits it further under load, `buckling.rs`).
pub const LONGEST: f64 = 1.0;
/// A joint closer than this to a member's node uses that node (m).
const SAME_NODE: f64 = 0.01;
/// Sides of a round part's prism.
const SIDES: usize = 12;

/// A part's cross-section along its length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Profile {
    /// Round, tapering from one diameter at its start to another at its end (m): a log, a pole.
    Round { start: f64, end: f64 },
    /// A board or a beam: `width` along its up, `depth` across both (m).
    Board { width: f64, depth: f64 },
}

/// How a part moves as a rigid body: about `centre`, with `velocity` there and `spin`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    pub centre: DVec3,
    pub velocity: DVec3,
    pub spin: DVec3,
}

impl Motion {
    /// The velocity of the part's point `at`.
    pub fn at(&self, at: DVec3) -> DVec3 {
        self.velocity + self.spin.cross(at - self.centre)
    }
}

/// A part, where it lies (its axis from `start` to `end`, its `up` across it).
#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub start: DVec3,
    pub end: DVec3,
    pub up: DVec3,
    pub profile: Profile,
    pub material: Material,
    pub motion: Motion,
}

impl Part {
    fn length(&self) -> f64 {
        (self.end - self.start).length()
    }

    fn at(&self, s: f64) -> DVec3 {
        self.start + (self.end - self.start) * (s / self.length())
    }

    fn section(&self, s: f64) -> Section {
        match self.profile {
            Profile::Round { start, end } => {
                Section::solid_round(start + (end - start) * (s / self.length()))
            }
            Profile::Board { width, depth } => Section::rectangle(width, depth),
        }
    }

    /// The corners of its prism from `s0` to `s1` along it.
    fn prism(&self, s0: f64, s1: f64) -> Vec<DVec3> {
        let axis = (self.end - self.start).normalize();
        let y = (self.up - axis * axis.dot(self.up)).normalize_or(axis.any_orthonormal_vector());
        let z = axis.cross(y);
        let mut corners = Vec::new();
        for s in [s0, s1] {
            let c = self.at(s);
            match self.profile {
                Profile::Round { start, end } => {
                    let r = 0.5 * (start + (end - start) * (s / self.length()));
                    for k in 0..SIDES {
                        let a = std::f64::consts::TAU * k as f64 / SIDES as f64;
                        let (sin, cos) = creeer_math::det::sin_cos(a);
                        corners.push(c + (y * cos + z * sin) * r);
                    }
                }
                Profile::Board { width, depth } => {
                    for (u, v) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                        corners.push(c + y * (0.5 * width * u) + z * (0.5 * depth * v));
                    }
                }
            }
        }
        corners
    }
}

/// Two parts fastened: `a` and `b` (indices), where on each axis (m from its start) the
/// fasteners cross it, and the joint's law.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fastening {
    pub a: usize,
    pub b: usize,
    pub on_a: f64,
    pub on_b: f64,
    pub law: JointLaw,
}

/// The structure, its collision shapes, and each part's nodes (m along it, and the node).
#[derive(Debug)]
pub struct Assembled {
    pub structure: Structure,
    pub shapes: Vec<(NodeId, Shape)>,
    pub nodes: Vec<Vec<(f64, NodeId)>>,
}

/// `parts` fastened by `fastenings`, as one structure.
///
/// # Errors
/// If a fastening names no part, lies off its part, or joins two points on one axis.
pub fn assemble(parts: &[Part], fastenings: &[Fastening]) -> Result<Assembled, String> {
    // Where each part needs a node: its ends, its joints, and every LONGEST between.
    let mut stations: Vec<Vec<f64>> = parts.iter().map(|p| vec![0.0, p.length()]).collect();
    for f in fastenings {
        for (part, s) in [(f.a, f.on_a), (f.b, f.on_b)] {
            let p = parts.get(part).ok_or_else(|| format!("no part {part}"))?;
            if !(0.0..=p.length()).contains(&s) {
                return Err(format!("a joint {s} m along a {} m part", p.length()));
            }
            stations[part].push(s);
        }
    }
    let mut b = StructureBuilder::new();
    let mut nodes = Vec::with_capacity(parts.len());
    for (part, s) in parts.iter().zip(&mut stations) {
        s.sort_by(f64::total_cmp);
        s.dedup_by(|x, y| (*x - *y).abs() < SAME_NODE);
        let mut along = Vec::new();
        for pair in s.windows(2) {
            let pieces = ((pair[1] - pair[0]) / LONGEST).ceil().max(1.0);
            // (A part of a few metres: a handful of pieces, whole and positive.)
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let count = pieces as usize;
            for k in 0..count {
                along.push(pair[0] + (pair[1] - pair[0]) * k as f64 / pieces);
            }
        }
        along.push(part.length());
        nodes.push(
            along
                .into_iter()
                .map(|s| (s, b.node(part.at(s))))
                .collect::<Vec<_>>(),
        );
    }
    let mut shapes = Vec::new();
    for (part, at) in parts.iter().zip(&nodes) {
        for pair in at.windows(2) {
            let ((s0, n0), (s1, n1)) = (pair[0], pair[1]);
            let mid = 0.5 * (s0 + s1);
            b.beam_oriented(n0, n1, part.section(mid), part.material, part.up);
            for (node, from, to) in [(n0, s0, mid), (n1, mid, s1)] {
                let origin = part.at(if node == n0 { s0 } else { s1 });
                let local: Vec<DVec3> = part.prism(from, to).iter().map(|c| *c - origin).collect();
                let (hull, centroid) = Hull::new(&local).map_err(|e| format!("{e:?}"))?;
                shapes.push((node, Shape::Hull(Arc::new(hull.about(-centroid)))));
            }
        }
    }
    let nearest = |part: usize, s: f64| {
        nodes[part]
            .iter()
            .min_by(|x, y| (x.0 - s).abs().total_cmp(&(y.0 - s).abs()))
            .map(|n| n.1)
    };
    for f in fastenings {
        let (Some(na), Some(nb)) = (nearest(f.a, f.on_a), nearest(f.b, f.on_b)) else {
            return Err("a part with no nodes".into());
        };
        if na == nb {
            return Err("a joint from a part to itself".into());
        }
        b.fasten(na, nb, &f.law);
    }
    let mut structure = b.build();
    for (part, at) in parts.iter().zip(&nodes) {
        for &(s, n) in at {
            structure.velocity[n] = part.motion.at(part.at(s));
            structure.angular_velocity[n] = part.motion.spin;
        }
    }
    Ok(Assembled {
        structure,
        shapes,
        nodes,
    })
}
