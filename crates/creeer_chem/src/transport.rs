//! Gases' viscosity and thermal conductivity, pure and mixed, at low pressure.
//!
//! - **Viscosity** by Chapman–Enskog with a Lennard-Jones potential: η = 26.69·√(M·T)/(σ²·Ω)
//!   µP (M in g/mol, σ in Å), the collision integral Ω(2,2)* by Neufeld, Janzen and Aziz's fit
//!   (J. Chem. Phys. 57, 1972, 1100). Parameters from GRI-Mech 3.0's transport data. Steam by
//!   IAPWS's dilute-gas formula (2008): its polarity puts plain Lennard-Jones 30 % off.
//! - **Thermal conductivity** by the modified Eucken correction, λ = (η/M)·(1.32·c_v + 1.77·R)
//!   (Poling, Prausnitz and O'Connell, §10-3); steam by IAPWS's dilute-gas formula (2011).
//!   Measured against reference data: viscosities within 2.5 %; conductivities of monatomic
//!   gases, hydrogen and steam within 1 %, of N₂, O₂, CO₂ and CH₄ 4–8 % high (the modified
//!   Eucken form's known bias for polyatomic gases; Eucken's own is as much low).
//! - **Mixtures** by Wilke's rule for the viscosity and the same weights (Mason and Saxena)
//!   for the conductivity.

use crate::system::System;
use creeer_materials::species::R;
use creeer_math::det;

/// Lennard-Jones σ (Å) and ε/k (K) by gas, GRI-Mech 3.0 (gri30_tran.dat).
pub const LENNARD_JONES: [(&str, f64, f64); 14] = [
    ("H2", 2.920, 38.0),
    ("O2", 3.458, 107.4),
    ("N2", 3.621, 97.53),
    ("CO", 3.650, 98.1),
    ("CO2", 3.763, 244.0),
    ("CH4", 3.746, 141.4),
    ("Ar", 3.330, 136.5),
    ("He", 2.576, 10.2),
    ("NH3", 2.920, 481.0),
    ("NO", 3.621, 97.53),
    ("OH", 2.750, 80.0),
    ("H", 2.050, 145.0),
    ("O", 2.750, 80.0),
    ("H2O", 2.605, 572.4),
];

/// Neufeld et al.'s Ω(2,2)* at reduced temperature `t` (T·k/ε), 0.3 ≤ T* ≤ 100 to 0.1 %.
fn omega22(t: f64) -> f64 {
    1.161_45 * det::pow(t, -0.148_74)
        + 0.524_87 * det::exp(-0.773_20 * t)
        + 2.161_78 * det::exp(-2.437_87 * t)
}

/// Steam's dilute-gas viscosity (Pa·s), IAPWS 2008: η₀ = 100·√T̄ / Σ Hᵢ/T̄ⁱ µPa·s.
fn steam_viscosity(t: f64) -> f64 {
    let tr = t / 647.096;
    let h = [1.677_52, 2.204_62, 0.636_656_4, -0.241_605];
    let sum: f64 = h
        .iter()
        .enumerate()
        .map(|(i, hi)| hi / det::pow(tr, i as f64))
        .sum();
    100.0 * tr.sqrt() / sum * 1e-6
}

/// Steam's dilute-gas thermal conductivity (W/(m·K)), IAPWS 2011: λ₀ = √T̄ / Σ Lᵢ/T̄ⁱ mW/(m·K).
fn steam_conductivity(t: f64) -> f64 {
    let tr = t / 647.096;
    let l = [
        2.443_221e-3,
        1.323_095e-2,
        6.770_357e-3,
        -3.454_586e-3,
        4.096_266e-4,
    ];
    let sum: f64 = l
        .iter()
        .enumerate()
        .map(|(i, li)| li / det::pow(tr, i as f64))
        .sum();
    tr.sqrt() / sum * 1e-3
}

/// Gas `j` of `sys`'s viscosity (Pa·s) and thermal conductivity (W/(m·K)) at `t`, if its
/// parameters are known.
pub fn pure(sys: &System, j: usize, t: f64) -> Option<(f64, f64)> {
    let s = &sys.species[j];
    let m = s.molar_mass;
    let cv = s.cp(t) - R;
    if s.name == "H2O" {
        return Some((steam_viscosity(t), steam_conductivity(t)));
    }
    let &(_, sigma, epsilon) = LENNARD_JONES.iter().find(|p| p.0 == s.name)?;
    let eta = 26.69 * (m * 1e3 * t).sqrt() / (sigma * sigma * omega22(t / epsilon)) * 1e-7;
    Some((eta, eta / m * (1.32 * cv + 1.77 * R)))
}

/// A gas mixture's viscosity (Pa·s) and thermal conductivity (W/(m·K)) with `moles` of each
/// species at `t`: Wilke's rule over the gases whose parameters are known (the others, traces
/// at most where this is asked, are left out).
pub fn mixture(sys: &System, moles: &[f64], t: f64) -> (f64, f64) {
    let known: Vec<(f64, f64, f64, f64)> = (0..sys.gases)
        .filter(|&j| moles[j] > 0.0)
        .filter_map(|j| {
            pure(sys, j, t).map(|(eta, lambda)| (moles[j], sys.species[j].molar_mass, eta, lambda))
        })
        .collect();
    let total: f64 = known.iter().map(|k| k.0).sum();
    let phi = |i: &(f64, f64, f64, f64), j: &(f64, f64, f64, f64), a: f64, b: f64| -> f64 {
        let root = 1.0 + (a / b).sqrt() * det::pow(j.1 / i.1, 0.25);
        root * root / (8.0 * (1.0 + i.1 / j.1)).sqrt()
    };
    let (mut eta, mut lambda) = (0.0, 0.0);
    for i in &known {
        let (den_eta, den_lambda) = known.iter().fold((0.0, 0.0), |(e, l), j| {
            (
                e + j.0 / total * phi(i, j, i.2, j.2),
                l + j.0 / total * phi(i, j, i.2, j.2),
            )
        });
        eta += i.0 / total * i.2 / den_eta;
        lambda += i.0 / total * i.3 / den_lambda;
    }
    (eta, lambda)
}
