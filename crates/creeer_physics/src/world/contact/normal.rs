//! The contacts' normal forces: each contact's law at its penetration (`law.rs`, `patch.rs`).

use super::{Contacts, Law, MATRIX_DEPTH};
use crate::structure::group::MemberState;
use std::sync::{Mutex, MutexGuard, PoisonError};

impl Contacts {
    /// The gaps of all contacts at `states`.
    pub(super) fn gaps(&self, states: &[MemberState<'_>]) -> Vec<f64> {
        (0..self.list.len())
            .map(|i| self.gap_and_slip(i, states).0)
            .collect()
    }

    /// The stored energy of the normal forces at `gaps` (and, over curved ground, the depths
    /// `measured` at each patch's points, `Self::measured_depths`), each contact's normal
    /// force (N), and if `points`, the forces at each patch's measured points (N; none on a
    /// plane; only the gradient needs them).
    pub(super) fn normal_forces(
        &self,
        gaps: &[f64],
        measured: &[Vec<f64>],
        points: bool,
    ) -> (f64, Vec<f64>, Vec<Vec<f64>>) {
        let mut forces = vec![0.0; gaps.len()];
        let mut energy = 0.0;
        for (i, c) in self.list.iter().enumerate() {
            if let Law::Hertz(h) = c.law {
                energy += h.energy(-gaps[i]);
                forces[i] = h.force(-gaps[i]);
            }
        }
        // A patch's law minimises over the face's motion (`patch.rs`): the line search's
        // energy at a state and the gradient there after ask for the same depths twice.
        let mut memo = self.memo.lock();
        memo.resize_with(self.patches.len(), || None);
        let mut at_points = Vec::with_capacity(self.patches.len());
        for ((p, last), measured) in self.patches.iter().zip(memo.iter_mut()).zip(measured) {
            let depths: [f64; 4] = std::array::from_fn(|k| -gaps[p.first + k]);
            #[allow(clippy::float_cmp)] // the very same depths
            let same = last
                .as_ref()
                .is_some_and(|r| r.depths == depths && r.measured == *measured);
            if !same {
                let (energy, forces, sigma) = match last {
                    Some(r) => {
                        p.evaluate_from(depths, measured, p.predict((r.sigma, r.depths), depths))
                    }
                    None => p.evaluate_on(depths, measured),
                };
                *last = Some(Remembered {
                    depths,
                    measured: measured.clone(),
                    energy,
                    forces,
                    sigma,
                    points: None,
                });
            }
            let r = last.as_mut().expect("remembered");
            energy += r.energy;
            forces[p.first..p.first + 4].copy_from_slice(&r.forces);
            if points {
                let sigma = r.sigma;
                at_points.push(
                    r.points
                        .get_or_insert_with(|| p.point_forces(depths, measured, sigma))
                        .clone(),
                );
            }
        }
        (energy, forces, at_points)
    }

    /// The stiffness of contact `i`'s normal force for the step's matrix: the law's at the
    /// penetration the step is expected to end at. A contact pressed in at the start rests
    /// near its depth there (taking the deeper prediction without contact overstated a
    /// resting Hertz contact's stiffness: a bridge on its piers needed 19 iterations instead
    /// of 10); one about to close may reach the prediction or a step's travel at its
    /// approach speed.
    pub(super) fn matrix_stiffness(&self, i: usize, gap_start: f64, h: f64) -> f64 {
        let c = &self.list[i];
        match c.law {
            Law::Hertz(law) => {
                let depth = if gap_start < 0.0 {
                    -gap_start
                } else {
                    (-c.gap_hat).max(-c.approach * h)
                };
                law.stiffness(depth.max(MATRIX_DEPTH))
            }
            Law::Patch { patch, vertex } => self.patches[patch].vertex_stiffness(vertex),
        }
    }
}

/// A patch's last result: its corner depths and measured ones, energy, corner forces and
/// the forces at its measured points.
#[derive(Debug)]
pub(super) struct Remembered {
    depths: [f64; 4],
    measured: Vec<f64>,
    energy: f64,
    forces: [f64; 4],
    sigma: creeer_math::glam::DVec3,
    points: Option<Vec<f64>>,
}

/// The last result of each patch's law. Valid for one step (a patch's search starts from
/// where the step began); a copy starts empty.
#[derive(Debug, Default)]
pub(super) struct Memo(Mutex<Vec<Option<Remembered>>>);

impl Clone for Memo {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Memo {
    fn lock(&self) -> MutexGuard<'_, Vec<Option<Remembered>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
