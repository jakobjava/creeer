//! AC grids: balanced three-phase networks as phasors at their frequency, solved
//! quasi-statically each tick (a 60 Hz tick cannot follow a 50 Hz waveform; its phasor is
//! what a grid's behaviour over a tick is).
//!
//! The network is the per-phase equivalent (each bus's line-to-neutral voltage): sources
//! and powers are given three-phase, as grids state them, and reported so.
//!
//! - **Branches**: lines (series impedance), shunts (capacitor banks, constant-impedance
//!   loads), sources (an EMF behind an impedance, or ideal), transformers (a series impedance
//!   and a magnetizing branch on the primary, an ideal ratio), constant-power loads (current
//!   injections conj(S/V), iterated), induction motors ([`crate::induction`]).
//! - **Solve**: the bus admittance matrix on the complex sparse LU, factorised when an
//!   impedance changes (a motor's slip), kept across the constant-power iterations.

use crate::induction::InductionMotor;
use creeer_math::sparse::{Assembly, Complex, Csc, Lu};
use creeer_network::{Ledger, Sum};

/// A bus of a grid; [`NEUTRAL`] is the reference.
pub type Bus = usize;
/// The neutral (0 V).
pub const NEUTRAL: Bus = 0;

pub(crate) const ROOT3: f64 = 1.732_050_807_568_877_2;

/// A branch of a grid.
#[derive(Clone, Debug)]
pub enum Branch {
    /// A series impedance between two buses (Ω per phase).
    Line { a: Bus, b: Bus, z: Complex },
    /// An admittance from a bus to neutral (S per phase).
    Shunt { a: Bus, y: Complex },
    /// An EMF (line-to-line, V, as a phasor) behind an impedance (Ω per phase; zero: ideal).
    Source { a: Bus, emf: Complex, z: Complex },
    /// A transformer, primary `a`, secondary `b`: ratio (primary over secondary voltage), its
    /// winding impedance referred to the primary, its magnetizing admittance at the primary.
    Transformer {
        a: Bus,
        b: Bus,
        ratio: f64,
        z: Complex,
        y: Complex,
    },
    /// A load drawing constant power (three-phase W + j·VAr) whatever its voltage.
    Load { a: Bus, power: Complex },
    /// An induction motor, its rotor's speed (rad/s) and the torque its load brakes it with.
    Motor {
        a: Bus,
        model: InductionMotor,
        speed: f64,
        load: f64,
    },
}

/// The factorised admittance matrix, and what it was made with.
#[derive(Debug)]
pub(crate) struct Solver {
    pub assembly: Assembly,
    pub matrix: Csc<Complex>,
    pub lu: Option<Lu<Complex>>,
    pub factored: Vec<Complex>,
    /// Unknowns: the buses' voltages, then each ideal source's current.
    pub size: usize,
}

/// An AC grid.
#[derive(Debug)]
pub struct Grid {
    /// Its frequency (Hz).
    pub frequency: f64,
    pub(crate) buses: usize,
    pub(crate) branches: Vec<Branch>,
    /// Each bus's voltage (line-to-neutral phasor, V).
    pub(crate) voltage: Vec<Complex>,
    /// The power each branch took in over the last step (three-phase W + j·VAr).
    pub(crate) power: Vec<Complex>,
    pub(crate) ledger: Ledger,
    pub(crate) solver: Option<Solver>,
}

impl Grid {
    /// An empty grid at `frequency` Hz (50 or 60).
    pub fn new(frequency: f64) -> Self {
        Self {
            frequency,
            buses: 1,
            branches: Vec::new(),
            voltage: vec![Complex::default()],
            power: Vec::new(),
            ledger: Ledger::new(0.0),
            solver: None,
        }
    }

    /// A new bus.
    pub fn bus(&mut self) -> Bus {
        self.buses += 1;
        self.voltage.push(Complex::default());
        self.solver = None;
        self.buses - 1
    }

    /// Adds a branch; returns its index.
    pub fn add(&mut self, branch: Branch) -> usize {
        self.ledger.joined(stored(&branch));
        self.branches.push(branch);
        self.power.push(Complex::default());
        self.solver = None;
        self.branches.len() - 1
    }

    /// The impedance of an inductance `henries` at this grid's frequency (Ω).
    pub fn reactance(&self, henries: f64) -> Complex {
        Complex::new(0.0, 2.0 * std::f64::consts::PI * self.frequency * henries)
    }

    /// The impedance of a capacitance `farads` at this grid's frequency (Ω).
    pub fn capacitive(&self, farads: f64) -> Complex {
        Complex::new(
            0.0,
            -1.0 / (2.0 * std::f64::consts::PI * self.frequency * farads),
        )
    }

    /// Changes the frequency (a sweep; the matrix is remade).
    pub fn set_frequency(&mut self, hz: f64) {
        self.frequency = hz;
    }

    /// A bus's line-to-line voltage magnitude (V) and its angle (rad).
    pub fn voltage(&self, bus: Bus) -> (f64, f64) {
        let v = self.voltage[bus];
        (v.abs() * ROOT3, v.arg())
    }

    /// The power a branch took in over the last step (three-phase W + j·VAr; a source's is
    /// negative: it gives).
    pub fn power(&self, branch: usize) -> Complex {
        self.power[branch]
    }

    /// A motor's speed (rad/s).
    pub fn speed(&self, branch: usize) -> f64 {
        match self.branches[branch] {
            Branch::Motor { speed, .. } => speed,
            _ => 0.0,
        }
    }

    /// The torque braking a motor's rotor (N·m).
    pub fn set_load(&mut self, branch: usize, torque: f64) {
        if let Branch::Motor { load, .. } = &mut self.branches[branch] {
            *load = torque;
        }
    }

    /// The energy it holds (J): its motors' rotors.
    pub fn stored(&self) -> f64 {
        let mut total = Sum::default();
        for b in &self.branches {
            total.add(stored(b));
        }
        total.value()
    }

    /// Where its energy went.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Every branch's admittance stamps (constant-power loads excluded), in a fixed order.
    /// Returns the unknowns' count.
    pub(crate) fn stamps(
        &self,
        mut put: impl FnMut(Option<usize>, Option<usize>, Complex),
    ) -> usize {
        let at = |b: Bus| (b != NEUTRAL).then(|| b - 1);
        let one = Complex::from(1.0);
        let n = self.buses - 1;
        let mut extra = n;
        for k in 0..n {
            put(Some(k), Some(k), Complex::new(crate::mna::GMIN, 0.0));
        }
        for b in &self.branches {
            match *b {
                Branch::Line { a, b, z } => {
                    let (y, a, b) = (one / z, at(a), at(b));
                    put(a, a, y);
                    put(b, b, y);
                    put(a, b, -y);
                    put(b, a, -y);
                }
                Branch::Shunt { a, y } => put(at(a), at(a), y),
                Branch::Source { a, z, .. } if z.abs() > 0.0 => put(at(a), at(a), one / z),
                Branch::Source { a, .. } => {
                    // Ideal: its current is an unknown, its voltage a constraint.
                    let e = Some(extra);
                    extra += 1;
                    put(at(a), e, one);
                    put(e, at(a), one);
                }
                Branch::Transformer { a, b, ratio, z, y } => {
                    let (ys, a, b) = (one / z, at(a), at(b));
                    put(a, a, ys + y);
                    put(b, b, ys * Complex::from(ratio * ratio));
                    put(a, b, ys * Complex::from(-ratio));
                    put(b, a, ys * Complex::from(-ratio));
                }
                Branch::Load { .. } => {}
                Branch::Motor {
                    a, model, speed, ..
                } => put(
                    at(a),
                    at(a),
                    one / model.impedance(model.slip(speed, self.frequency)),
                ),
            }
        }
        extra
    }

    /// The matrix for the present impedances, factorised if it changed.
    ///
    /// # Panics
    /// If the matrix is singular (GMIN ties every bus to neutral: it is not).
    pub(crate) fn factorise(&mut self) {
        let mut values = Vec::new();
        let size = self.stamps(|r, c, v| {
            if r.is_some() && c.is_some() {
                values.push(v);
            }
        });
        if self.solver.is_none() {
            let mut positions = Vec::new();
            self.stamps(|r, c, _| {
                if let (Some(r), Some(c)) = (r, c) {
                    positions.push((r, c));
                }
            });
            let assembly = Assembly::new(size, &positions);
            self.solver = Some(Solver {
                matrix: assembly.zeros(),
                assembly,
                lu: None,
                factored: Vec::new(),
                size,
            });
        }
        let s = self.solver.as_mut().expect("made");
        if s.lu.is_none() || values != s.factored {
            s.assembly.fill(&values, &mut s.matrix.values);
            let regular = "a grid's matrix is regular";
            match &mut s.lu {
                Some(lu) => {
                    lu.refactor(&s.matrix).expect(regular);
                }
                None => s.lu = Some(Lu::new(&s.matrix).expect(regular)),
            }
            s.factored = values;
        }
    }
}

/// What a branch holds (J).
fn stored(b: &Branch) -> f64 {
    match *b {
        Branch::Motor { model, speed, .. } => 0.5 * model.inertia * speed * speed,
        _ => 0.0,
    }
}
