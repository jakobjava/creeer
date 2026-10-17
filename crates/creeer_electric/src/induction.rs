//! The three-phase induction motor: its per-phase equivalent circuit (IEEE's: stator R₁ + jX₁,
//! magnetizing jX_m, rotor R₂/s + jX₂ referred to the stator), slip from its rotor's speed.

use creeer_math::sparse::Complex;

/// An induction motor, star-connected (its parameters per phase, Ω).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InductionMotor {
    pub r1: f64,
    pub x1: f64,
    pub xm: f64,
    pub r2: f64,
    pub x2: f64,
    /// Poles (4: 1500 rpm synchronous at 50 Hz).
    pub poles: f64,
    /// Rotor and load inertia (kg·m²), viscous friction (N·m·s/rad).
    pub inertia: f64,
    pub friction: f64,
}

impl InductionMotor {
    /// A 7.5 kW, 400 V, 4-pole, 50 Hz motor (typical equivalent circuit for its size: rated
    /// near 1450 rpm, breakdown torque about 3× rated, starting about 1.5×).
    pub const KW7_5: Self = Self {
        r1: 0.6,
        x1: 1.2,
        xm: 40.0,
        r2: 0.5,
        x2: 1.2,
        poles: 4.0,
        inertia: 0.05,
        friction: 0.01,
    };

    /// Synchronous speed at `frequency` Hz (rad/s).
    pub fn synchronous(&self, frequency: f64) -> f64 {
        4.0 * std::f64::consts::PI * frequency / self.poles
    }

    /// Slip at rotor speed `speed` (rad/s).
    pub fn slip(&self, speed: f64, frequency: f64) -> f64 {
        let ws = self.synchronous(frequency);
        (ws - speed) / ws
    }

    /// The rotor branch's impedance at slip `s` (Ω), none at synchronous speed (it carries no
    /// current there).
    fn rotor(&self, s: f64) -> Option<Complex> {
        (s.abs() > 1e-12).then(|| Complex::new(self.r2 / s, self.x2))
    }

    /// The impedance a phase sees at slip `s` (Ω).
    pub fn impedance(&self, s: f64) -> Complex {
        let stator = Complex::new(self.r1, self.x1);
        let magnetizing = Complex::new(0.0, self.xm);
        stator
            + match self.rotor(s) {
                Some(rotor) => magnetizing * rotor / (magnetizing + rotor),
                None => magnetizing,
            }
    }

    /// The torque (N·m) at slip `s` fed `v` (line-to-neutral phasor, V) at `frequency` Hz:
    /// 3·|I₂|²·R₂/(s·ω_s), the air-gap power over the synchronous speed.
    pub fn torque(&self, v: Complex, s: f64, frequency: f64) -> f64 {
        let Some(rotor) = self.rotor(s) else {
            return 0.0;
        };
        let i1 = v / self.impedance(s);
        let magnetizing = Complex::new(0.0, self.xm);
        let i2 = i1 * magnetizing / (magnetizing + rotor);
        3.0 * i2.norm_squared() * self.r2 / (s * self.synchronous(frequency))
    }
}
