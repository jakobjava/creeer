//! Assembling the system matrix of a structure for the sparse solver.
//!
//! Each node has two 3×3 blocks of unknowns: its translation (block index = node) and its
//! rotation (block index = node count + node). An element couples all four blocks of its
//! two nodes: 6 off-diagonal blocks per element.

use super::Structure;
use super::element::MEASURES;
use crate::sparse::{SymbolicCholesky, nested_dissection_tree};
use creeer_math::glam::{DMat3, DVec3};

/// Index pairs (into an element's four dof groups) of its 6 off-diagonal blocks.
const PAIRS: [(usize, usize); 6] = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];

impl Structure {
    /// Block index of an element's dof group (0: x_a, 1: θ_a, 2: x_b, 3: θ_b).
    pub(crate) fn block(&self, a: usize, b: usize, group: usize) -> usize {
        let n = self.node_count();
        [a, n + a, b, n + b][group]
    }

    /// Free (1) / fixed (0) axes of a block.
    pub(crate) fn block_mask(&self, block: usize) -> DVec3 {
        let n = self.node_count();
        if block < n {
            self.free_translation[block]
        } else {
            self.free_rotation[block - n]
        }
    }

    fn block_edges(&self) -> Vec<(usize, usize)> {
        let mut edges = Vec::with_capacity(PAIRS.len() * self.elements.len());
        for e in &self.elements {
            for (p, q) in PAIRS {
                edges.push((self.block(e.a, e.b, p), self.block(e.a, e.b, q)));
            }
        }
        edges
    }

    /// Pattern analysis for this structure's system matrix. The nodes are ordered by
    /// geometric nested dissection; each node's rotation block follows its translation block.
    pub(crate) fn symbolic(&self) -> SymbolicCholesky {
        let n = self.node_count();
        let node_edges: Vec<(usize, usize)> = self.elements.iter().map(|e| (e.a, e.b)).collect();
        let (order, tree) = nested_dissection_tree(&self.rest, &node_edges);
        let perm = order.iter().flat_map(|&node| [node, n + node]).collect();
        SymbolicCholesky::with_dissection(&self.block_edges(), perm, tree.scaled(2))
    }

    /// Fills `diag` and `off` (layout of [`Self::symbolic`]) with
    /// `mass_scale·M + Σ stiffness_scale(element)·K_element`, then replaces fixed axes by
    /// identity rows. K is that of the rest shape, or with `current` the Gauss-Newton
    /// stiffness Σ Bᵀ·K·B at the current positions and orientations (always positive
    /// semi-definite; it leaves out the stress-dependent geometric stiffness).
    pub(crate) fn assemble(
        &self,
        current: bool,
        mass_scale: f64,
        stiffness_scale: impl Fn(usize) -> f64,
        diag: &mut [DMat3],
        off: &mut [DMat3],
    ) {
        let n = self.node_count();
        let rotations: Vec<DMat3> = if current {
            self.orientation
                .iter()
                .map(|q| DMat3::from_quat(*q))
                .collect()
        } else {
            vec![DMat3::IDENTITY; n]
        };
        let positions = if current { &self.position } else { &self.rest };
        for i in 0..n {
            diag[i] = DMat3::IDENTITY * (mass_scale * self.mass[i]);
            diag[n + i] = rotations[i] * self.inertia[i] * rotations[i].transpose() * mass_scale;
        }
        let outer = |u: DVec3, v: DVec3| DMat3::from_cols(u * v.x, u * v.y, u * v.z);
        for (index, e) in self.elements.iter().enumerate() {
            let (_, g) = e.kinematics(
                positions[e.a],
                rotations[e.a],
                positions[e.b],
                rotations[e.b],
            );
            let scale = stiffness_scale(index);
            // Block (p, q) of the element matrix: Σ_mn K_mn · g_m[p] · g_n[q]ᵀ.
            let block = |p: usize, q: usize| {
                let mut sum = DMat3::ZERO;
                for m in 0..MEASURES {
                    for k in 0..MEASURES {
                        let kmn = e.stiffness.entry(m, k);
                        if kmn != 0.0 {
                            sum += outer(g[m][p], g[k][q]) * kmn;
                        }
                    }
                }
                sum * scale
            };
            for group in 0..4 {
                diag[self.block(e.a, e.b, group)] += block(group, group);
            }
            for (k, (p, q)) in PAIRS.into_iter().enumerate() {
                off[PAIRS.len() * index + k] = block(p, q);
            }
        }
        // Fixed axes: keep only the free part and put 1 on the fixed diagonal.
        let project = |block: usize| DMat3::from_diagonal(self.block_mask(block));
        for (b, d) in diag.iter_mut().enumerate() {
            let p = project(b);
            *d = p * *d * p + (DMat3::IDENTITY - p);
        }
        for (index, e) in self.elements.iter().enumerate() {
            for (k, (p, q)) in PAIRS.into_iter().enumerate() {
                let (bp, bq) = (self.block(e.a, e.b, p), self.block(e.a, e.b, q));
                let slot = &mut off[PAIRS.len() * index + k];
                *slot = project(bp) * *slot * project(bq);
            }
        }
    }

    /// Number of off-diagonal blocks in the system matrix.
    pub(crate) fn off_block_count(&self) -> usize {
        PAIRS.len() * self.elements.len()
    }
}

impl Structure {
    /// For every block, the (element, dof group) pairs acting on it, in ascending element
    /// order (CSR layout: pairs of block b are `incidence[ptr[b]..ptr[b + 1]]`).
    pub(crate) fn incidence(&self) -> (Vec<usize>, Vec<(u32, u8)>) {
        let blocks = 2 * self.node_count();
        let mut lists: Vec<Vec<(u32, u8)>> = vec![Vec::new(); blocks];
        for (index, e) in self.elements.iter().enumerate() {
            let index = u32::try_from(index).expect("fewer than 4 billion elements");
            for group in 0..4u8 {
                lists[self.block(e.a, e.b, usize::from(group))].push((index, group));
            }
        }
        let mut ptr = Vec::with_capacity(blocks + 1);
        ptr.push(0);
        let mut flat = Vec::new();
        for list in lists {
            flat.extend(list);
            ptr.push(flat.len());
        }
        (ptr, flat)
    }
}
