//! A patch's stiffness for the step's matrices (`patch.rs`).

use super::Patch;
use creeer_math::glam::{DMat3, DVec3};

impl Patch {
    /// The stiffness of corner `k` for the step's matrices (N/m): its share of the face's,
    /// the force it takes when the whole face is pushed in. A corner far from where a
    /// clipped face presses is lifted by that push (the face tilts): it gets none. (Its
    /// negative share made its dashpot NaN, and a pile's step stopped at its prediction,
    /// crates inside each other, F5.)
    pub fn vertex_stiffness(&self, k: usize) -> f64 {
        self.matrix[k].iter().sum::<f64>().max(0.0)
    }

    /// The stiffness coupling corners `k` and `l` for the step's matrices (N/m).
    pub fn coupling(&self, k: usize, l: usize) -> f64 {
        self.matrix[k][l]
    }

    /// The stiffness between the corners when the whole domain presses, as the face moves
    /// rigidly: the material's K = κ·∫NₖNₗ in series with the limit, K − K·B·(L + Bᵀ·K·B)⁻¹·
    /// Bᵀ·K, seen through the face's rigid motions (P·K·P, P the projection onto them). A
    /// rigid face cannot twist, and the material's stiffness against twisting (a hundred
    /// times the limit) would only give its corners' dashpots a stiffness no motion has.
    pub(super) fn effective(&self, layer: DMat3) -> [[f64; 4]; 4] {
        let k: [[f64; 4]; 4] = self.mass.map(|row| row.map(|m| m * self.modulus));
        self.in_series(k, layer)
    }

    /// The material's stiffness `k` between the corners in series with the limit `layer`
    /// (when capped), seen through the face's rigid motions.
    fn in_series(&self, k: [[f64; 4]; 4], layer: DMat3) -> [[f64; 4]; 4] {
        let b = |c: usize| DVec3::from(self.rigid[c]);
        // Kᵦ = K·B (4 × 3), Bᵀ·K·B (3 × 3).
        let kb: [DVec3; 4] = std::array::from_fn(|r| (0..4).map(|c| b(c) * k[r][c]).sum());
        let btkb = (0..4).fold(DMat3::ZERO, |m, r| {
            m + DMat3::from_cols(b(r) * kb[r].x, b(r) * kb[r].y, b(r) * kb[r].z)
        });
        let series: [[f64; 4]; 4] = if self.capped() {
            let inverse = (layer + btkb).inverse();
            std::array::from_fn(|r| std::array::from_fn(|c| k[r][c] - kb[r].dot(inverse * kb[c])))
        } else {
            k
        };
        // P = B·(Bᵀ·B)⁻¹·Bᵀ.
        let btb = (0..4).fold(DMat3::ZERO, |m, r| {
            m + DMat3::from_cols(b(r) * b(r).x, b(r) * b(r).y, b(r) * b(r).z)
        });
        let inverse = btb.inverse();
        let p: [[f64; 4]; 4] =
            std::array::from_fn(|r| std::array::from_fn(|c| b(r).dot(inverse * b(c))));
        let ps: [[f64; 4]; 4] = std::array::from_fn(|r| {
            std::array::from_fn(|c| (0..4).map(|m| p[r][m] * series[m][c]).sum())
        });
        std::array::from_fn(|r| std::array::from_fn(|c| (0..4).map(|m| ps[r][m] * p[m][c]).sum()))
    }

    /// The patch energy's stiffness between the corners at corner depths `depths` (and the
    /// depths `measured` over curved ground): the material pressed there, in series with the
    /// limit as loaded there. Where `effective` takes the whole face as pressed, this is the
    /// exact Hessian: a face pressed along an edge is far softer against tilting than a whole
    /// one, and the whole face's stiffness in the matrix made Newton creep (F6).
    /// `sigma`: the face's motion there if `found`, else where its search starts.
    pub fn tangent(
        &self,
        depths: [f64; 4],
        measured: &[f64],
        (sigma, found): (DVec3, bool),
    ) -> [[f64; 4]; 4] {
        let offsets = self.offsets(depths, measured);
        if !self.capped() {
            return self.in_series(self.material(depths, &offsets).2, DMat3::ZERO);
        }
        let sigma = if found {
            sigma
        } else {
            self.minimise(depths, &offsets, sigma).2
        };
        let layer = self.layer.at(sigma).2;
        let shifted: [f64; 4] =
            std::array::from_fn(|k| depths[k] - DVec3::from(self.rigid[k]).dot(sigma));
        self.in_series(self.material(shifted, &offsets).2, layer)
    }

    /// The material's stiffness between the corners at corner depths `depths` (N/m; κ·∫NₖNₗ
    /// over the part pressed in there).
    pub fn stiffness_at(&self, depths: [f64; 4]) -> [[f64; 4]; 4] {
        self.material(depths, &[]).2
    }
}
