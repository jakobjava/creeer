//! What makes gases and liquids not ideal, for the substances it matters for: a gas's
//! second virial coefficient (its fugacity, ln φ = B·p/RT) by the Tsonopoulos correlation,
//! and a liquid's molar volume (its density, and the Poynting factor on its fugacity) by
//! the Rackett equation in the Yamada–Gunn form, anchored at a measured density (water's by
//! IAPWS's saturated density instead).
//!
//! Critical constants, acentric factors and Rackett parameters: B. E. Poling, J. M.
//! Prausnitz and J. P. O'Connell, The Properties of Gases and Liquids, 5th ed. (2001),
//! appendix A; water's critical point IAPWS-95. Tsonopoulos: AIChE J. 20 (1974) 263, polar
//! terms from C. Tsonopoulos and J. L. Heidman, Fluid Phase Equilibria 57 (1990) 261.
//! Densities at 298.15 K: CRC Handbook.

use creeer_materials::species::R;
use creeer_math::det;

/// A substance's constants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Critical {
    /// Its gas species' name in the species data.
    pub gas: &'static str,
    /// Critical temperature (K) and pressure (Pa), acentric factor.
    pub tc: f64,
    pub pc: f64,
    pub omega: f64,
    /// Tsonopoulos's polar terms a and b (zero for normal fluids), and whether the
    /// correlation holds for it at all: not for the alcohols, whose hydrogen bonds need polar
    /// terms of their own (left ideal here rather than guessed).
    pub a: f64,
    pub b: f64,
    pub virial: bool,
    /// Rackett's Z_RA, and the liquid's density at 298.15 K (kg/m³; 0 if no liquid then).
    pub z_ra: f64,
    pub density: f64,
}

/// The substances known.
pub const SUBSTANCES: [Critical; 12] = [
    Critical {
        gas: "H2O",
        tc: 647.096,
        pc: 22.064e6,
        omega: 0.3443,
        a: -0.0109,
        b: 0.0,
        virial: true,
        z_ra: 0.2338,
        density: 997.05,
    },
    Critical {
        gas: "C6H6",
        tc: 562.05,
        pc: 4.895e6,
        omega: 0.2103,
        a: 0.0,
        b: 0.0,
        virial: true,
        z_ra: 0.2698,
        density: 873.6,
    },
    Critical {
        gas: "C7H8",
        tc: 591.75,
        pc: 4.108e6,
        omega: 0.2640,
        a: 0.0,
        b: 0.0,
        virial: true,
        z_ra: 0.2644,
        density: 862.2,
    },
    Critical {
        gas: "C2H5OH",
        tc: 513.92,
        pc: 6.148e6,
        omega: 0.6490,
        a: 0.0,
        b: 0.0,
        virial: false,
        z_ra: 0.2502,
        density: 785.1,
    },
    Critical {
        gas: "CH3OH",
        tc: 512.64,
        pc: 8.097e6,
        omega: 0.5658,
        a: 0.0,
        b: 0.0,
        virial: false,
        z_ra: 0.2334,
        density: 786.6,
    },
    Critical {
        gas: "C8H18,n-octane",
        tc: 568.70,
        pc: 2.490e6,
        omega: 0.3996,
        a: 0.0,
        b: 0.0,
        virial: true,
        z_ra: 0.2571,
        density: 698.6,
    },
    Critical {
        gas: "CO2",
        tc: 304.13,
        pc: 7.3773e6,
        omega: 0.2236,
        a: 0.0,
        b: 0.0,
        virial: true,
        z_ra: 0.2722,
        density: 0.0,
    },
    // Ammonia as a normal fluid: its polar term is not here, so its |B| is low.
    Critical {
        gas: "NH3",
        tc: 405.40,
        pc: 11.333e6,
        omega: 0.2526,
        a: 0.0,
        b: 0.0,
        virial: true,
        z_ra: 0.2465,
        density: 0.0,
    },
    Critical {
        gas: "N2",
        tc: 126.19,
        pc: 3.3958e6,
        omega: 0.0372,
        a: 0.0,
        b: 0.0,
        virial: true,
        z_ra: 0.2900,
        density: 0.0,
    },
    Critical {
        gas: "O2",
        tc: 154.58,
        pc: 5.043e6,
        omega: 0.0222,
        a: 0.0,
        b: 0.0,
        virial: true,
        z_ra: 0.2900,
        density: 0.0,
    },
    Critical {
        gas: "CH4",
        tc: 190.56,
        pc: 4.599e6,
        omega: 0.0115,
        a: 0.0,
        b: 0.0,
        virial: true,
        z_ra: 0.2892,
        density: 0.0,
    },
    Critical {
        gas: "Ar",
        tc: 150.69,
        pc: 4.863e6,
        omega: -0.0022,
        a: 0.0,
        b: 0.0,
        virial: true,
        z_ra: 0.2924,
        density: 0.0,
    },
];

/// Saturated liquid water's density at `t` (kg/m³): IAPWS's auxiliary equation (W. Wagner
/// and A. Pruß, J. Phys. Chem. Ref. Data 22, 1993, 783). Rackett's, which holds for most
/// liquids to 2 %, is 4 % low for water at 373 K and 9 % at 550 K (its hydrogen bonds).
pub fn water_density(t: f64) -> f64 {
    let tau = 1.0 - t / 647.096;
    let p = |e: f64| det::pow(tau, e);
    322.0
        * (1.0 + 1.992_740_64 * p(1.0 / 3.0) + 1.099_653_42 * p(2.0 / 3.0)
            - 0.510_839_303 * p(5.0 / 3.0)
            - 1.754_934_79 * p(16.0 / 3.0)
            - 45.517_035_2 * p(43.0 / 3.0)
            - 6.746_944_50e5 * p(110.0 / 3.0))
}

/// The constants of the substance whose gas is `gas`.
pub fn critical(gas: &str) -> Option<&'static Critical> {
    SUBSTANCES.iter().find(|c| c.gas == gas)
}

impl Critical {
    /// The second virial coefficient at `t` (m³/mol): B·p_c/(R·T_c) = f⁰ + ω·f¹ + f²
    /// (Tsonopoulos). `None` where the correlation does not hold for it.
    pub fn second_virial(&self, t: f64) -> Option<f64> {
        if !self.virial {
            return None;
        }
        let tr = self.tc / t;
        let (tr2, tr3, tr6) = (tr * tr, tr * tr * tr, det::pow(tr, 6.0));
        let tr8 = tr6 * tr2;
        let f0 = 0.1445 - 0.330 * tr - 0.1385 * tr2 - 0.0121 * tr3 - 0.000_607 * tr8;
        let f1 = 0.0637 + 0.331 * tr2 - 0.423 * tr3 - 0.008 * tr8;
        let f2 = self.a * tr6 - self.b * tr8;
        Some((f0 + self.omega * f1 + f2) * R * self.tc / self.pc)
    }

    /// The liquid's molar volume at `t` (m³/mol), from its density at 298.15 K and
    /// Rackett's temperature dependence: V = V_ref·Z_RA^((1 − T_r)^(2/7) − (1 − T_r,ref)^(2/7)).
    /// `None` without a liquid density.
    pub fn liquid_volume(&self, t: f64, molar_mass: f64) -> Option<f64> {
        if self.density <= 0.0 || t >= self.tc {
            return None;
        }
        if self.gas == "H2O" {
            return Some(molar_mass / water_density(t));
        }
        let tau = |t: f64| det::pow(1.0 - t / self.tc, 2.0 / 7.0);
        let reference = molar_mass / self.density;
        Some(reference * det::pow(self.z_ra, tau(t) - tau(298.15)))
    }
}
