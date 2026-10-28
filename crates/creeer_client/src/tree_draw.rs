//! How trees are drawn (G1.7, ADR-0031; `tree_view.rs` gathers them): stems and branches as
//! rods in the bark's colour, the foliage as rough masses in the leaves'.

use crate::physics_sandbox::PhysicsSnapshot;
use creeer_math::glam::{DQuat, DVec3};
use creeer_sim::trees::{Part, Tree};

/// The stem of `tree` between heights `span` as `n` rods, each its radius at its middle, placed
/// by `place` (tree frame to world).
pub(crate) fn rods(
    out: &mut PhysicsSnapshot,
    tree: &Tree,
    (from, to): (f64, f64),
    place: impl Fn(DVec3) -> DVec3,
    colour: [u8; 3],
    n: usize,
) {
    #[allow(clippy::cast_precision_loss)]
    let step = (to - from) / n as f64;
    for k in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let (a, b) = (from + step * k as f64, from + step * (k + 1) as f64);
        let r = tree.radius(0.5 * (a + b)).max(0.005);
        out.rods.push((
            place(DVec3::new(0.0, 0.0, a)),
            place(DVec3::new(0.0, 0.0, b)),
            r,
            colour,
        ));
    }
}

/// `part`'s branches of `tree`, each a rod and its foliage a mass along it.
pub(crate) fn branches(
    out: &mut PhysicsSnapshot,
    tree: &Tree,
    part: &Part,
    (place, turn): (impl Fn(DVec3) -> DVec3, DQuat),
    (bark, leaves): ([u8; 3], [u8; 3]),
) {
    let kept = tree.kept(part);
    for &(k, b) in &kept {
        let node = &tree.nodes[k];
        let branch = &node.branches[b];
        let d = branch.direction();
        let base = DVec3::new(0.0, 0.0, node.z);
        out.rods.push((
            place(base),
            place(base + d * branch.length),
            0.6 * branch.radius,
            bark,
        ));
        // A lone branch's foliage along it; a whorl's as one mass about the stem (below).
        if node.branches.len() == 1 {
            let along = DQuat::from_rotation_arc(DVec3::X, turn * d);
            let half = DVec3::new(0.45, 0.22, 0.1) * branch.length;
            out.blobs.push((
                place(base + d * (0.55 * branch.length)),
                along,
                half,
                leaves,
            ));
        }
    }
    let mut whorls: Vec<usize> = kept
        .iter()
        .map(|kb| kb.0)
        .filter(|&k| tree.nodes[k].branches.len() > 1)
        .collect();
    whorls.dedup();
    for k in whorls {
        let node = &tree.nodes[k];
        let reach = (kept.iter().filter(|kb| kb.0 == k))
            .map(|kb| {
                let b = &node.branches[kb.1];
                creeer_math::det::sin(b.angle) * b.length
            })
            .fold(0.0, f64::max);
        let half = DVec3::new(0.85 * reach, 0.85 * reach, 0.18 * reach.max(0.5));
        out.blobs
            .push((place(DVec3::new(0.0, 0.0, node.z)), turn, half, leaves));
    }
}

/// `stem`'s crown as `layers` masses stacked up it, each as wide as the outline there.
pub(crate) fn crown(
    out: &mut PhysicsSnapshot,
    stem: &Tree,
    species: &creeer_sim::trees::Species,
    (place, turn): (impl Fn(DVec3) -> DVec3, DQuat),
    layers: usize,
) {
    let length = stem.height - stem.crown_base;
    let widest = species.crown.radius.0 + species.crown.radius.1 * stem.dbh;
    #[allow(clippy::cast_precision_loss)]
    let thick = length / layers as f64;
    for k in 0..layers {
        #[allow(clippy::cast_precision_loss)]
        let z = stem.crown_base + thick * (k as f64 + 0.5);
        let reach = if layers == 1 {
            0.7 * widest
        } else {
            widest * species.crown.outline.reach((stem.height - z) / length)
        };
        let half = DVec3::new(reach.max(0.2), reach.max(0.2), 0.65 * thick);
        out.blobs
            .push((place(DVec3::new(0.0, 0.0, z)), turn, half, species.leaves));
    }
}
