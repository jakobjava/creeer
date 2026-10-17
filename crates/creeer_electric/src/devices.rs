//! The nonlinear and stateful devices: diodes and LEDs, batteries, solar modules, DC
//! machines. Each gives its parameters with its sources.

use creeer_materials::Curve;
use creeer_math::det;
use std::sync::Arc;

/// Boltzmann's constant over the elementary charge (V/K): the thermal voltage per kelvin.
pub const K_OVER_Q: f64 = 8.617_333_262e-5;

/// A junction diode (Shockley's law with series resistance), or an LED.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Diode {
    /// Saturation current I_s (A) and emission coefficient n.
    pub saturation: f64,
    pub emission: f64,
    /// Series resistance R_s (Ω).
    pub series: f64,
    /// The fraction of its power an LED gives off as light (its wall-plug efficiency); 0 for
    /// a plain diode.
    pub light: f64,
}

impl Diode {
    /// The 1N4148 small-signal diode (its SPICE model as Fairchild/onsemi publish it:
    /// IS = 2.682 nA, N = 1.836, RS = 0.5664 Ω).
    pub const N4148: Self = Self {
        saturation: 2.682e-9,
        emission: 1.836,
        series: 0.5664,
        light: 0.0,
    };
    /// The 1N4007 rectifier (its common SPICE model: IS = 7.03 nA, N = 1.8, RS = 34 mΩ).
    pub const N4007: Self = Self {
        saturation: 7.03e-9,
        emission: 1.8,
        series: 0.034,
        light: 0.0,
    };
    /// A 5 mm red LED: 1.9 V at 20 mA, 2.0 V at 30 mA (I_s and n fitted to those), a fifth of
    /// its power as light (red AlInGaP).
    pub const RED_LED: Self = Self {
        saturation: 1.0e-18,
        emission: 1.95,
        series: 2.0,
        light: 0.2,
    };

    /// The junction's current (A) and its slope (S) at junction voltage `v` and temperature
    /// `t` (K).
    pub fn junction(&self, v: f64, t: f64) -> (f64, f64) {
        let nvt = self.emission * K_OVER_Q * t;
        let e = det::exp(v / nvt);
        (self.saturation * (e - 1.0), self.saturation * e / nvt)
    }

    /// Newton's junction voltage limited (SPICE's pnjlim): above the critical voltage, a
    /// step up of more than 2·n·V_T is taken on the logarithm instead, so the exponential
    /// cannot overflow and Newton converges from any start.
    pub fn limit(&self, new: f64, old: f64, t: f64) -> f64 {
        let nvt = self.emission * K_OVER_Q * t;
        let critical = nvt * det::ln(nvt / (std::f64::consts::SQRT_2 * self.saturation));
        if new > critical && (new - old).abs() > 2.0 * nvt {
            if old > 0.0 {
                let arg = 1.0 + (new - old) / nvt;
                if arg > 0.0 {
                    old + nvt * det::ln(arg)
                } else {
                    critical
                }
            } else {
                nvt * det::ln(new / nvt)
            }
        } else {
            new
        }
    }
}

/// A battery: a kinetic battery model (Manwell & McGowan 1993) for its charge, a Thevenin
/// circuit for its voltage.
///
/// - Its charge sits in two wells: what the electrodes can give at once (a fraction `c` of
///   it) and what must first diffuse to them; the bound charge flows to the available as
///   k·(h₂ − h₁), the wells' heights the charge over their size. Drawn hard, the available
///   well empties first and the voltage collapses with charge still inside: the rate effect
///   Peukert's law describes, without losing a coulomb (rested, the battery recovers it).
/// - Its terminal voltage: the open-circuit voltage at the available well's level, less
///   R₀·I and an RC pair's voltage (charge transfer and diffusion, τ = R₁·C₁).
#[derive(Clone, Debug, PartialEq)]
pub struct Battery {
    /// Open-circuit voltage (V) against state of charge (0–1).
    pub ocv: Arc<Curve>,
    /// Capacity (C).
    pub capacity: f64,
    /// Series resistance R₀ (Ω), and the RC pair (Ω, F).
    pub r0: f64,
    pub r1: f64,
    pub c1: f64,
    /// The available well's share `c`, and the rate constant k (1/s) between the wells.
    pub available: f64,
    pub rate: f64,
    /// Below this terminal voltage its protection disconnects it (V).
    pub cutoff: f64,
}

impl Battery {
    /// A 3.2 Ah lithium-ion 18650 cell (NCA chemistry, as the Panasonic NCR18650B): OCV from
    /// 4.19 V full to 3.0 V near empty, plunging as the electrodes' surface empties (the
    /// available well's last percent: that collapse is what ends a discharge), 45 mΩ in
    /// series, an RC pair of 20 mΩ and 1 kF (τ = 20 s),
    /// cut off at 2.5 V.
    pub fn li_ion_18650() -> Self {
        let points = [
            (0.0, 2.50),
            (0.01, 3.00),
            (0.03, 3.25),
            (0.06, 3.40),
            (0.10, 3.47),
            (0.20, 3.56),
            (0.30, 3.62),
            (0.40, 3.68),
            (0.50, 3.74),
            (0.60, 3.82),
            (0.70, 3.90),
            (0.80, 3.98),
            (0.90, 4.07),
            (1.00, 4.19),
        ];
        Self {
            ocv: Arc::new(Curve::new(points.to_vec()).expect("increasing")),
            capacity: 3.2 * 3600.0,
            r0: 0.045,
            r1: 0.020,
            c1: 1000.0,
            available: 0.9,
            rate: 2.0e-4,
            cutoff: 2.5,
        }
    }

    /// A 12 V 100 Ah flooded lead-acid battery: OCV 11.8 V empty to 12.7 V full, 5 mΩ, a
    /// third of its charge available at once (lead-acid's strong rate effect: C/20 gives its
    /// rating, C/1 about 60 % of it), cut off at 10.5 V.
    pub fn lead_acid_12v_100ah() -> Self {
        Self {
            ocv: Arc::new(
                Curve::new(vec![
                    (0.0, 10.50),
                    (0.02, 11.80),
                    (0.5, 12.25),
                    (1.0, 12.70),
                ])
                .expect("increasing"),
            ),
            capacity: 100.0 * 3600.0,
            r0: 0.005,
            r1: 0.004,
            c1: 50_000.0,
            available: 0.35,
            rate: 2.7e-4,
            cutoff: 10.5,
        }
    }

    /// The open-circuit voltage at the available well's level `h1` (0–1).
    pub fn open_circuit(&self, h1: f64) -> f64 {
        self.ocv.at(h1.clamp(0.0, 1.0))
    }

    /// The chemical energy (J) in a well of size `size` (fraction of the capacity) filled to
    /// height `h` (0–1): ∫ OCV dq.
    pub fn energy(&self, size: f64, h: f64) -> f64 {
        size * self.capacity * self.ocv.integral(0.0, h.clamp(0.0, 1.0))
    }
}

/// A solar module: the single-diode model, I = I_pv − I₀·(e^((V + R_s·I)/(a·N_s·V_T)) − 1)
/// − (V + R_s·I)/R_p, with I_pv and I₀ following irradiance and temperature (Villalva,
/// Gazoli & Filho, IEEE Trans. Power Electronics 24(5), 2009, eqs. 3–7).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolarModule {
    /// Cells in series.
    pub cells: f64,
    /// At standard test conditions (1000 W/m², 25 °C): short-circuit current (A), open-circuit
    /// voltage (V), and their temperature coefficients (A/K, V/K).
    pub short_circuit: f64,
    pub open_circuit: f64,
    pub current_coefficient: f64,
    pub voltage_coefficient: f64,
    /// Diode ideality a, series and parallel resistances (Ω).
    pub ideality: f64,
    pub series: f64,
    pub parallel: f64,
    /// Area (m²) and the fraction of light it absorbs (the rest is reflected).
    pub area: f64,
}

/// Standard test conditions: irradiance (W/m²) and cell temperature (K).
pub const STC: (f64, f64) = (1000.0, 298.15);

impl SolarModule {
    /// The Kyocera KC200GT (200 W, 54 cells), Villalva et al.'s Table II parameters.
    pub const KC200GT: Self = Self {
        cells: 54.0,
        short_circuit: 8.21,
        open_circuit: 32.9,
        current_coefficient: 0.0032,
        voltage_coefficient: -0.1230,
        ideality: 1.3,
        series: 0.221,
        parallel: 415.405,
        area: 1.425 * 0.990,
    };

    /// I_pv (A) and I₀ (A) at irradiance `g` (W/m²) and cell temperature `t` (K)
    /// (Villalva's eqs. 4 and 7; I_pv,n ≈ I_sc,n·(R_p + R_s)/R_p).
    pub fn sources(&self, g: f64, t: f64) -> (f64, f64) {
        let dt = t - STC.1;
        let ipv_n = self.short_circuit * (self.parallel + self.series) / self.parallel;
        let ipv = (ipv_n + self.current_coefficient * dt) * g / STC.0;
        let vt = self.cells * K_OVER_Q * t;
        let i0 = (self.short_circuit + self.current_coefficient * dt)
            / (det::exp(
                (self.open_circuit + self.voltage_coefficient * dt) / (self.ideality * vt),
            ) - 1.0);
        (ipv, i0)
    }

    /// The diode's a·N_s·V_T at temperature `t` (V).
    pub fn thermal_voltage(&self, t: f64) -> f64 {
        self.ideality * self.cells * K_OVER_Q * t
    }
}

/// A permanent-magnet DC machine: back-EMF k·ω, torque k·I (k in V·s/rad = N·m/A), its
/// armature R_a and L_a, its rotor's inertia and viscous friction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DcMachine {
    /// Torque (and back-EMF) constant k.
    pub constant: f64,
    /// Armature resistance (Ω) and inductance (H).
    pub resistance: f64,
    pub inductance: f64,
    /// Rotor inertia (kg·m²) and viscous friction (N·m·s/rad).
    pub inertia: f64,
    pub friction: f64,
}

impl DcMachine {
    /// A 24 V, 250 W brushed motor (an e-bike or scooter's): no-load 3000 rpm, k = 0.074,
    /// R_a = 0.25 Ω, L_a = 0.4 mH.
    pub const SCOOTER_24V: Self = Self {
        constant: 0.074,
        resistance: 0.25,
        inductance: 4.0e-4,
        inertia: 4.0e-4,
        friction: 2.0e-5,
    };
}
