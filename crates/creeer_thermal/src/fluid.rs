//! The fluids parts exchange heat with by convection, and their properties against
//! temperature (and, for gases, pressure):
//!
//! - **air:** density from the ideal-gas law; viscosity, conductivity and specific heat from
//!   Incropera's Table A.4 (100–1000 K), beyond it viscosity and conductivity following
//!   Sutherland's laws (White, *Viscous Fluid Flow*, ±2 % to 1900 K) and specific heat that
//!   of undissociated air from the JANAF tables;
//! - **water:** liquid at 1 atm, Incropera's Table A.6 (saturated water, 0–100 °C); below 4 °C
//!   it expands as it cools (its expansion coefficient is negative), so buoyancy turns over.
//!
//! (The species database of F6.5 will compute any mixture's; these are enough for a part
//! cooling in the wind or in the sea.)

use creeer_math::det;

/// Molar mass of dry air (kg/mol).
pub const MOLAR_MASS: f64 = 0.028_965;
/// The gas constant (J/(mol·K)).
pub const R: f64 = 8.314_462_618;

/// The fluid around a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Medium {
    /// Dry air.
    Air,
    /// Liquid water.
    Water,
}

impl Medium {
    /// Its properties at temperature `t` (K) and pressure `p` (Pa).
    pub fn at(self, t: f64, p: f64) -> Fluid {
        match self {
            Self::Air => Fluid::air(t, p),
            Self::Water => Fluid::water(t),
        }
    }
}

/// Sutherland's law, (T/T0)^(3/2)·(T0 + S)/(T + S).
fn sutherland(t: f64, t0: f64, s: f64) -> f64 {
    det::pow(t / t0, 1.5) * (t0 + s) / (t + s)
}

/// A fluid's properties at one state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fluid {
    /// Density (kg/m³).
    pub density: f64,
    /// Dynamic viscosity (Pa·s).
    pub viscosity: f64,
    /// Thermal conductivity (W/(m·K)).
    pub conductivity: f64,
    /// Specific heat at constant pressure (J/(kg·K)).
    pub specific_heat: f64,
    /// Volumetric thermal expansion coefficient (1/K): 1/T for an ideal gas; negative for
    /// water below 4 °C.
    pub expansion: f64,
}

/// Incropera, *Fundamentals of Heat and Mass Transfer*, Table A.4 (air at 1 atm; viscosity
/// and conductivity hardly depend on pressure): (T K, μ·10⁷ Pa·s, k·10³ W/(m·K), cp J/(kg·K)).
const AIR: [(f64, f64, f64, f64); 19] = [
    (100.0, 71.1, 9.34, 1032.0),
    (150.0, 103.4, 13.8, 1012.0),
    (200.0, 132.5, 18.1, 1007.0),
    (250.0, 159.6, 22.3, 1006.0),
    (300.0, 184.6, 26.3, 1007.0),
    (350.0, 208.2, 30.0, 1009.0),
    (400.0, 230.1, 33.8, 1014.0),
    (450.0, 250.7, 37.3, 1021.0),
    (500.0, 270.1, 40.7, 1030.0),
    (550.0, 288.4, 43.9, 1040.0),
    (600.0, 305.8, 46.9, 1051.0),
    (650.0, 322.5, 49.7, 1063.0),
    (700.0, 338.8, 52.4, 1075.0),
    (750.0, 354.6, 54.9, 1087.0),
    (800.0, 369.8, 57.3, 1099.0),
    (850.0, 384.3, 59.6, 1110.0),
    (900.0, 398.1, 62.0, 1121.0),
    (950.0, 411.3, 64.3, 1131.0),
    (1000.0, 424.4, 66.7, 1141.0),
];

/// Specific heat of undissociated air beyond the table, from the JANAF tables' N₂, O₂ and Ar.
const HOT_AIR_SPECIFIC_HEAT: [(f64, f64); 3] =
    [(1000.0, 1141.0), (1500.0, 1210.0), (2000.0, 1250.0)];

/// Incropera Table A.6, saturated liquid water: (T K, ρ kg/m³, cp J/(kg·K), μ·10⁶ Pa·s,
/// k·10³ W/(m·K), β·10⁶ 1/K).
type WaterRow = (f64, f64, f64, f64, f64, f64);

const WATER: [WaterRow; 22] = [
    (273.15, 1000.0, 4217.0, 1750.0, 569.0, -68.05),
    (275.0, 1000.0, 4211.0, 1652.0, 574.0, -32.74),
    (280.0, 1000.0, 4198.0, 1422.0, 582.0, 46.04),
    (285.0, 1000.0, 4189.0, 1225.0, 590.0, 114.1),
    (290.0, 999.0, 4184.0, 1080.0, 598.0, 174.0),
    (295.0, 998.0, 4181.0, 959.0, 606.0, 227.5),
    (300.0, 997.0, 4179.0, 855.0, 613.0, 276.1),
    (305.0, 995.0, 4178.0, 769.0, 620.0, 320.6),
    (310.0, 993.0, 4178.0, 695.0, 628.0, 361.9),
    (315.0, 991.0, 4179.0, 631.0, 634.0, 400.4),
    (320.0, 989.0, 4180.0, 577.0, 640.0, 436.7),
    (325.0, 987.0, 4182.0, 528.0, 645.0, 471.2),
    (330.0, 984.0, 4184.0, 489.0, 650.0, 504.0),
    (335.0, 982.0, 4186.0, 453.0, 656.0, 535.5),
    (340.0, 979.0, 4188.0, 420.0, 660.0, 566.0),
    (345.0, 977.0, 4191.0, 389.0, 664.0, 595.4),
    (350.0, 974.0, 4195.0, 365.0, 668.0, 624.2),
    (355.0, 971.0, 4199.0, 343.0, 671.0, 652.3),
    (360.0, 967.0, 4203.0, 324.0, 674.0, 697.9),
    (365.0, 963.0, 4209.0, 306.0, 677.0, 707.1),
    (370.0, 961.0, 4214.0, 289.0, 679.0, 728.7),
    (373.15, 958.0, 4217.0, 279.0, 680.0, 750.1),
];

/// Interpolates column `f` of rows `rows` at `t`, held at the ends.
fn interpolate<const N: usize, R: Copy>(
    rows: &[R; N],
    key: impl Fn(R) -> f64,
    f: impl Fn(R) -> f64,
    t: f64,
) -> f64 {
    let k = rows.partition_point(|&r| key(r) <= t);
    match k {
        0 => f(rows[0]),
        k if k == N => f(rows[N - 1]),
        k => {
            let (a, b) = (rows[k - 1], rows[k]);
            f(a) + (f(b) - f(a)) * (t - key(a)) / (key(b) - key(a))
        }
    }
}

impl Fluid {
    /// Dry air at temperature `t` (K) and pressure `p` (Pa).
    pub fn air(t: f64, p: f64) -> Self {
        let (lo, hi) = (AIR[0], AIR[AIR.len() - 1]);
        // Beyond the table, Sutherland's laws carry its end values on.
        let (viscosity, conductivity) = if t < lo.0 {
            (
                lo.1 * 1e-7 * sutherland(t, lo.0, 110.4),
                lo.2 * 1e-3 * sutherland(t, lo.0, 194.0),
            )
        } else if t > hi.0 {
            (
                hi.1 * 1e-7 * sutherland(t, hi.0, 110.4),
                hi.2 * 1e-3 * sutherland(t, hi.0, 194.0),
            )
        } else {
            (
                interpolate(&AIR, |r| r.0, |r| r.1, t) * 1e-7,
                interpolate(&AIR, |r| r.0, |r| r.2, t) * 1e-3,
            )
        };
        let specific_heat = if t > hi.0 {
            interpolate(&HOT_AIR_SPECIFIC_HEAT, |r| r.0, |r| r.1, t)
        } else {
            interpolate(&AIR, |r| r.0, |r| r.3, t)
        };
        Self {
            density: p * MOLAR_MASS / (R * t),
            viscosity,
            conductivity,
            specific_heat,
            expansion: 1.0 / t,
        }
    }

    /// Liquid water at temperature `t` (K), held at the table's ends (ice and boiling are
    /// other domains': melting in the network's enthalpies, boiling in F6.5's equilibria).
    pub fn water(t: f64) -> Self {
        let at = |f: fn(WaterRow) -> f64| interpolate(&WATER, |r| r.0, f, t);
        Self {
            density: at(|r| r.1),
            specific_heat: at(|r| r.2),
            viscosity: at(|r| r.3) * 1e-6,
            conductivity: at(|r| r.4) * 1e-3,
            expansion: at(|r| r.5) * 1e-6,
        }
    }

    /// Kinematic viscosity ν (m²/s).
    pub fn kinematic_viscosity(&self) -> f64 {
        self.viscosity / self.density
    }

    /// Thermal diffusivity α (m²/s).
    pub fn diffusivity(&self) -> f64 {
        self.conductivity / (self.density * self.specific_heat)
    }

    /// Prandtl number.
    pub fn prandtl(&self) -> f64 {
        self.viscosity * self.specific_heat / self.conductivity
    }
}
