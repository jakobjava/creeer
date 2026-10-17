//! A planet's atmosphere: temperature, pressure, density, viscosity and the speed of sound
//! at any altitude, from its data (`creeer_terrain::AtmosphereData`).
//!
//! Temperature is piecewise linear in geopotential altitude H = R·z/(R + z) (the height a
//! constant surface gravity would need for the same potential: gravity falls as 1/r², and
//! on a small planet fast). Hydrostatic balance then has closed forms in each layer:
//! p = p_b·(T/T_b)^(−g₀M/(R·L)) where the lapse rate L ≠ 0, p = p_b·exp(−g₀M·(H − H_b)/(R·T_b))
//! where it is isothermal; above the top, isothermal at the top's temperature. Density is the
//! ideal gas's; the speed of sound √(γRT/M) with γ from the mixture's heat capacity, the
//! viscosity Wilke's mixture of its gases (`creeer_chem::transport`); both depend on the
//! temperature only, so they are tabulated by the kelvin.

use creeer_chem::System;
use creeer_materials::species::{R, SpeciesTable};
use creeer_math::det;
use creeer_terrain::AtmosphereData;

/// The air at one place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Air {
    /// Temperature (K), pressure (Pa), density (kg/m³).
    pub t: f64,
    pub p: f64,
    pub density: f64,
    /// Dynamic viscosity (Pa·s), speed of sound (m/s).
    pub viscosity: f64,
    pub sound: f64,
}

/// A layer: its base (geopotential altitude, m), the temperature (K) and pressure (Pa) there,
/// its lapse rate (K/m).
#[derive(Clone, Copy, Debug)]
struct Layer {
    base: f64,
    t: f64,
    p: f64,
    lapse: f64,
}

/// A planet's atmosphere.
#[derive(Clone, Debug)]
pub struct Atmosphere {
    radius: f64,
    g0: f64,
    /// Mean molar mass (kg/mol).
    pub molar_mass: f64,
    layers: Vec<Layer>,
    /// The temperatures tabulated (K, from 100 by the kelvin): viscosity, γ.
    table: Vec<(f64, f64)>,
}

/// The lowest temperature tabulated (K) and how many kelvins.
const TABLE_FROM: f64 = 100.0;
const TABLE_KELVINS: usize = 2000;

impl Atmosphere {
    /// The atmosphere of `data` on a planet of datum radius `radius` (m) and gravitational
    /// parameter `mu` (m³/s²), its gases from `table`.
    ///
    /// # Errors
    /// If a gas is not in the species table.
    pub fn new(
        data: &AtmosphereData,
        radius: f64,
        mu: f64,
        table: &SpeciesTable,
    ) -> Result<Self, String> {
        let names: Vec<&str> = data.composition.iter().map(|c| c.0.as_str()).collect();
        let sys = System::new(table, &names)?;
        let mut moles = vec![0.0; sys.species.len()];
        for (name, x) in &data.composition {
            moles[sys.index(name).ok_or(format!("no gas {name}"))?] = *x;
        }
        let molar_mass = sys.mass(&moles);
        let g0 = mu / (radius * radius);
        let mut layers = Vec::with_capacity(data.layers.len() + 1);
        let (mut base, mut t, mut p) = (0.0, data.surface_temperature, data.surface_pressure);
        for &(top, lapse) in &data.layers {
            layers.push(Layer { base, t, p, lapse });
            let t_top = t + lapse * (top - base);
            p = pressure(p, t, t_top, lapse, top - base, g0, molar_mass);
            (base, t) = (top, t_top);
        }
        // Above the last layer: isothermal.
        layers.push(Layer {
            base,
            t,
            p,
            lapse: 0.0,
        });
        let table = (0..TABLE_KELVINS)
            .map(|k| {
                let t = TABLE_FROM + k as f64;
                let (viscosity, _) = creeer_chem::transport::mixture(&sys, &moles, t);
                let cp = sys.heat_capacity(&moles, t);
                (viscosity, cp / (cp - R))
            })
            .collect();
        Ok(Self {
            radius,
            g0,
            molar_mass,
            layers,
            table,
        })
    }

    /// The geopotential altitude (m) of geometric altitude `z` (m) above the datum.
    pub fn geopotential(&self, z: f64) -> f64 {
        self.radius * z / (self.radius + z)
    }

    /// The air at geometric altitude `z` (m) above the datum.
    pub fn at(&self, z: f64) -> Air {
        let h = self.geopotential(z.max(0.0));
        let layer = self
            .layers
            .iter()
            .rev()
            .find(|l| h >= l.base)
            .unwrap_or(&self.layers[0]);
        let t = layer.t + layer.lapse * (h - layer.base);
        let p = pressure(
            layer.p,
            layer.t,
            t,
            layer.lapse,
            h - layer.base,
            self.g0,
            self.molar_mass,
        );
        let (viscosity, gamma) = self.tabulated(t);
        Air {
            t,
            p,
            density: p * self.molar_mass / (R * t),
            viscosity,
            sound: (gamma * R * t / self.molar_mass).sqrt(),
        }
    }

    /// Viscosity and γ at `t`, between the kelvins tabulated.
    fn tabulated(&self, t: f64) -> (f64, f64) {
        let x = (t - TABLE_FROM).clamp(0.0, (TABLE_KELVINS - 2) as f64);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // clamped to the table above
        let k = x.floor() as usize;
        let f = x - k as f64;
        let (a, b) = (self.table[k], self.table[k + 1]);
        (a.0 + f * (b.0 - a.0), a.1 + f * (b.1 - a.1))
    }
}

/// The pressure at the top of a span `dh` (geopotential, m) of a layer with lapse rate
/// `lapse` from `p0` at `t0` to `t1`.
fn pressure(p0: f64, t0: f64, t1: f64, lapse: f64, dh: f64, g0: f64, m: f64) -> f64 {
    if lapse.abs() < 1e-12 {
        p0 * det::exp(-g0 * m * dh / (R * t0))
    } else {
        p0 * det::pow(t1 / t0, -g0 * m / (R * lapse))
    }
}
