//! Heat across a joint: two surfaces pressed together touch only at the peaks of their
//! roughness, so a joint conducts far worse than solid metal. Its conductance is the
//! contacts' plus the gas's in the gaps between them (Yovanovich's plastic contact model;
//! Bahrami, Culham & Yovanovich 2004 review it against experiments: ±25 %).

use creeer_math::det;

/// Two surfaces in contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    /// The two materials' conductivities (W/(m·K)).
    pub conductivity: (f64, f64),
    /// The softer surface's microhardness (Pa): about 2–3× its Brinell hardness for metals
    /// (steel S235 ~ 1.5 GPa, aluminium ~ 0.9 GPa).
    pub hardness: f64,
    /// RMS roughnesses (m): machined ~ 1–3 µm, ground ~ 0.2–0.5 µm.
    pub roughness: (f64, f64),
    /// RMS asperity slopes (rad): ~ 0.1 machined, ~ 0.05 ground (Lambert & Fletcher's
    /// m ≈ 0.125·(σ/µm)^0.402 correlates them with roughness).
    pub slope: (f64, f64),
    /// The gas in the gaps: its conductivity (W/(m·K); air 0.026, vacuum 0) and its
    /// rarefaction length (m; air at 1 atm ~ 0.37 µm, growing as pressure falls).
    pub gas: (f64, f64),
}

impl Joint {
    /// Steel on steel, machined, in air: a bolted flange.
    pub fn machined_steel_in_air() -> Self {
        Self {
            conductivity: (50.0, 50.0),
            hardness: 1.5e9,
            roughness: (2.0e-6, 2.0e-6),
            slope: (0.1, 0.1),
            gas: (0.026, 0.37e-6),
        }
    }

    /// Its conductance per area (W/(m²·K)) pressed together at `pressure` Pa.
    pub fn conductance(&self, pressure: f64) -> f64 {
        let (k1, k2) = self.conductivity;
        let k = 2.0 * k1 * k2 / (k1 + k2);
        let sigma = det::hypot(self.roughness.0, self.roughness.1);
        let m = det::hypot(self.slope.0, self.slope.1);
        let p = (pressure / self.hardness).max(1e-12);
        // The contacts: 1.25·k·(m/σ)·(P/H)^0.95.
        let contacts = 1.25 * k * m / sigma * det::pow(p, 0.95);
        // The gaps: the mean plane separation Y = 1.53·σ·(P/H)^-0.097, with the gas's
        // rarefaction length added to it.
        let gap = 1.53 * sigma * det::pow(p, -0.097);
        contacts + self.gas.0 / (gap + self.gas.1)
    }
}
