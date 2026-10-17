//! The material under a patch (`patch.rs`): the pressure over the face's pressed part.

use super::super::integrals::{pressed_mass, square_integral};
use super::Patch;

impl Patch {
    /// The depth at the corners of triangle `t` of the domain for corner depths `depths`:
    /// the face's bilinear blend, plus how far the measured depth lies off it there
    /// (`offsets`, per triangle and corner; none on a plane).
    pub(super) fn depths_at(&self, t: usize, depths: [f64; 4], offsets: &[[f64; 3]]) -> [f64; 3] {
        let w = self.triangles[t].1;
        let off = offsets.get(t).copied().unwrap_or([0.0; 3]);
        std::array::from_fn(|j| (0..4).map(|k| w[j][k] * depths[k]).sum::<f64>() + off[j])
    }

    /// The material's energy and corner forces at corner depths `depths` (with `offsets`),
    /// and its stiffness between the corners there (κ·∫NₖNₗ over the pressed area).
    ///
    /// The pressure is κ·δ over the pressed part, κ the whole face's punch. (A punch of the
    /// pressed area's own stiffness, κ = 2·E*/√(π·A_p), was tried in F5: scale-free at
    /// corners, but a tall box sliding with its rear edge nearly lifted turned unstable in
    /// yaw, an initial 10⁻³ rad/s growing to 0.07 where the whole face's law damps it out.)
    pub(super) fn material(
        &self,
        depths: [f64; 4],
        offsets: &[[f64; 3]],
    ) -> (f64, [f64; 4], [[f64; 4]; 4]) {
        let mut forces = [0.0; 4];
        let mut pressed = [[0.0; 4]; 4];
        let mut energy = 0.0;
        let kappa = self.modulus;
        for (t, &(area, w)) in self.triangles.iter().enumerate() {
            let f = self.depths_at(t, depths, offsets);
            if f.iter().all(|&d| d <= 0.0) {
                continue;
            }
            let (integral, gradient) = square_integral(f, area);
            energy += 0.5 * kappa * integral;
            for k in 0..4 {
                for (j, wj) in w.iter().enumerate() {
                    forces[k] += 0.5 * kappa * gradient[j] * wj[k];
                }
            }
            // Pressed all over: the whole triangle's, found once (`Patch::new`).
            if f.iter().all(|&d| d > 0.0) {
                for (row, full) in pressed.iter_mut().zip(&self.full[t]) {
                    for (m, f) in row.iter_mut().zip(full) {
                        *m += kappa * f;
                    }
                }
                continue;
            }
            let p = pressed_mass(f, area);
            for k in 0..4 {
                for l in 0..4 {
                    for (a, wa) in w.iter().enumerate() {
                        for (b, wb) in w.iter().enumerate() {
                            pressed[k][l] += kappa * p[a][b] * wa[k] * wb[l];
                        }
                    }
                }
            }
        }
        (energy, forces, pressed)
    }

    /// Whether the whole domain is pressed in at corner depths `depths` (with `offsets`).
    pub(super) fn pressed_all_over(&self, depths: [f64; 4], offsets: &[[f64; 3]]) -> bool {
        (0..self.triangles.len())
            .all(|t| self.depths_at(t, depths, offsets).iter().all(|&d| d > 0.0))
    }
}
