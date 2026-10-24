//! The solver's matrix: built at the shape the structure has (or its rest shape), with the
//! stiffness of the contacts at its nodes, and factorised; rebuilt when the shape has moved
//! far from it, or the contacts changed it materially.

use super::Structure;
use super::solver::ImplicitSolver;
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// 3×3 blocks of the matrix by block (translations 0..n, rotations n..2n).
type Blocks = Vec<(usize, DMat3)>;

impl ImplicitSolver {
    /// Takes the stiffness that contacts add at nodes into the factorised matrix, if it changed
    /// materially since the last factorisation: other nodes touched, or a node's block changed
    /// by more than half. Without it a structure resting on contacts converges slowly (the
    /// matrix misses their stiffness; 48 iterations instead of 11 for a bridge on its piers).
    /// Nodes in `exempt` are left out of that judgement: a joint solve (`coupled.rs`) takes
    /// their contacts' stiffness as it is now, exactly, whatever the factorisation holds (a
    /// block landing on a bridge refactorised it in 23 of a tick's 34 impact substeps for its
    /// deck joints alone). Returns whether it refactorised.
    pub(super) fn use_contact_stiffness(
        &mut self,
        structure: &Structure,
        (contacts, air): (Blocks, Blocks),
        exempt: &[usize],
    ) -> bool {
        // A block of a node held fast is masked out of the matrix (`factor_inner`): it changes
        // nothing, and changing, it refactorised a 20,000-beam tower standing on its clamped
        // columns every few ticks, 2.2 s each (G2.8).
        let merged = |mut blocks: Vec<(usize, DMat3)>| {
            blocks.retain(|(block, _)| structure.block_mask(*block) != DVec3::ZERO);
            blocks.sort_by_key(|b| b.0);
            let mut out: Vec<(usize, DMat3)> = Vec::with_capacity(blocks.len());
            for (node, h) in blocks {
                match out.last_mut() {
                    Some(last) if last.0 == node => last.1 += h,
                    _ => out.push((node, h)),
                }
            }
            out
        };
        let (air, all) = (merged(air.clone()), merged([contacts, air].concat()));
        let norm = |m: DMat3| m.to_cols_array().iter().map(|v| v * v).sum::<f64>().sqrt();
        let judged = |list: &[(usize, DMat3)]| -> Vec<(usize, DMat3)> {
            list.iter()
                .filter(|b| !exempt.contains(&b.0))
                .copied()
                .collect()
        };
        // With no air or water on it, all its blocks judged as ever (the saved fixtures hold
        // that motion bit for bit): each changed by more than half its own.
        let same_blocks = |now: &[(usize, DMat3)], then: &[(usize, DMat3)]| {
            now.len() == then.len()
                && now.iter().zip(then).all(|((a, ha), (b, hb))| {
                    a == b && norm(*ha - *hb) <= 0.5 * norm(*ha).max(norm(*hb))
                })
        };
        let same = if air.is_empty() && self.air_blocks.is_empty() {
            same_blocks(&judged(&all), &judged(&self.contact_blocks))
        } else {
            // Air's and water's blocks change materially only by more than a hundredth of the
            // node's inertia in the matrix too (the blocks enter it times β²): a standing
            // tower's 6,000 dashpots, each a few newton-seconds a metre that double and halve
            // with the breeze, refactorised it every few ticks (G2.8).
            let beta2 = self.beta() * self.beta();
            let n = structure.node_count();
            let inertial = |block: usize| {
                let own = if block < n {
                    DMat3::IDENTITY * structure.mass[block]
                } else {
                    structure.inertia[block - n]
                };
                0.01 * norm(own) / beta2
            };
            let block_of = |list: &[(usize, DMat3)], k: usize| {
                (list.binary_search_by_key(&k, |b| b.0)).map_or(DMat3::ZERO, |i| list[i].1)
            };
            let (now, then) = (judged(&all), judged(&self.contact_blocks));
            let (air_now, air_then) = (judged(&air), judged(&self.air_blocks));
            let mut keys: Vec<usize> = now.iter().chain(&then).map(|b| b.0).collect();
            keys.sort_unstable();
            keys.dedup();
            keys.iter().all(|&k| {
                let (ha, hb) = (block_of(&now, k), block_of(&then, k));
                let change = norm(ha - hb);
                let of_air = norm(block_of(&air_now, k) - block_of(&air_then, k));
                // The contacts' share of the change judged as ever, the air's against inertia.
                change <= 0.5 * norm(ha).max(norm(hb))
                    || (change - of_air <= 0.5 * norm(ha).max(norm(hb)) && of_air <= inertial(k))
            })
        };
        if same {
            return false;
        }
        self.contact_blocks = all;
        self.air_blocks = air;
        self.refactor(structure);
        true
    }

    /// Rebuilds the matrix at the structure's current shape, e.g. after member properties
    /// changed.
    pub fn refactor(&mut self, structure: &Structure) {
        self.factor(structure, true);
    }

    /// Builds and factorises the matrix at the rest shape or the `current` one.
    ///
    /// # Panics
    /// If the matrix is not positive definite.
    pub(super) fn factor(&mut self, structure: &Structure, current: bool) {
        assert!(
            self.try_factor(structure, current),
            "matrix cannot be made positive definite"
        );
    }

    /// As [`Self::factor`]; false if the matrix cannot be made positive definite (negative
    /// masses or stiffnesses: never in a world that was stepped).
    pub(super) fn try_factor(&mut self, structure: &Structure, current: bool) -> bool {
        crate::profile::add(crate::profile::Slot::Factorisations, 1);
        let timer = crate::profile::start();
        let done = self.factor_inner(structure, current);
        timer.stop(crate::profile::Slot::Factorise);
        done
    }

    fn factor_inner(&mut self, structure: &Structure, current: bool) -> bool {
        self.factorisations += 1;
        self.unit_columns.clear();
        if current {
            self.factor_orientation.clone_from(&structure.orientation);
        } else {
            self.factor_orientation.fill(DQuat::IDENTITY);
        }
        self.factor_current = current;
        self.factor_position.clone_from(if current {
            &structure.position
        } else {
            &structure.rest
        });
        self.factor_broken.clear();
        self.factor_broken
            .extend(structure.elements.iter().map(|e| e.broken));
        let beta = self.beta();
        let blocks = 2 * structure.node_count();
        let mut diag = vec![DMat3::ZERO; blocks];
        let mut off = vec![DMat3::ZERO; structure.off_block_count()];
        let damping = |e: usize| beta * beta + beta * structure.elements[e].material.damping;
        structure.assemble(current, 1.0, damping, &mut diag, &mut off);
        for &(node, h) in &self.contact_blocks {
            let p = DMat3::from_diagonal(structure.block_mask(node));
            diag[node] += p * h * p * (beta * beta);
        }
        // In exact arithmetic M + (β² + βc)·K is positive definite. With a mechanism (parts
        // broken loose) and a huge β (statics) rounding can make it fail; the matrix is only
        // a preconditioner, so a slightly shifted one is just as good. Never panic.
        let scale = diag
            .iter()
            .map(|d| d.x_axis.x.max(d.y_axis.y).max(d.z_axis.z))
            .fold(0.0, f64::max);
        let mut shift = 0.0;
        while self.chol.factor(&diag, &off).is_err() {
            let next = if shift > 0.0 {
                shift * 100.0
            } else {
                1e-12 * scale
            };
            for d in &mut diag {
                *d += DMat3::IDENTITY * (next - shift);
            }
            // No progress (an empty or non-finite matrix), or past what rounding could need.
            if !(next.is_finite() && next > shift && next <= scale) {
                return false;
            }
            shift = next;
        }
        true
    }
}

impl ImplicitSolver {
    /// The preconditioner applied to the three unit translations at `node`, as
    /// [`Self::precondition`] gives them: R·A₀⁻¹·Rᵀ·e_k with each node's turn R since the
    /// factorisation. A₀⁻¹ of the unit vectors is solved once per factorisation and node;
    /// only the turns are applied anew (the same columns but for rounding, at a fraction of a
    /// solve: a block landing on a bridge rebuilt 27 of them in each of its 34 impact
    /// substeps a tick).
    pub(super) fn unit_columns(&mut self, s: &Structure, node: usize) -> [Vec<DVec3>; 3] {
        let n = s.node_count();
        let at = match self.unit_columns.iter().position(|c| c.0 == node) {
            Some(at) => at,
            None => {
                let solved = std::array::from_fn(|j| {
                    let mut column = vec![DVec3::ZERO; 2 * n];
                    column[node][j] = 1.0;
                    self.chol.solve(&mut column);
                    column
                });
                self.unit_columns.push((node, solved));
                self.unit_columns.len() - 1
            }
        };
        let (turns, columns) = (&self.turns, &self.unit_columns[at].1);
        let back = turns[node].transpose();
        std::array::from_fn(|k| {
            // Rᵀ·e_k is column k of Rᵀ.
            let w = back.col(k);
            (0..2 * n)
                .map(|b| {
                    turns[b % n] * (columns[0][b] * w.x + columns[1][b] * w.y + columns[2][b] * w.z)
                })
                .collect()
        })
    }
}
