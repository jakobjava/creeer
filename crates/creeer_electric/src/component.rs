//! What a circuit is made of: components between nodes, their parameters and their state.
//!
//! Every component is a branch between two terminals `a` and `b`; its current is counted
//! from `a` through it to `b`, its voltage as v_a − v_b, so v·i is the power it takes in (a
//! source's is negative). Node 0 is ground.

use crate::devices::{Battery, DcMachine, Diode, SolarModule};
use creeer_materials::Curve;
use std::sync::Arc;

/// A node of a circuit; [`GROUND`] is the reference, at 0 V.
pub type Node = usize;

/// The reference node.
pub const GROUND: Node = 0;

/// A component's kind and parameters.
#[derive(Clone, Debug)]
pub enum Kind {
    /// A resistor (Ω).
    Resistor(f64),
    /// A conductor of a material: R = ρ(T)·L/A, its temperature set from outside (the thermal
    /// network), so it heats and its resistance rises.
    Wire {
        /// The material's resistivity against temperature (Ω·m).
        resistivity: Arc<Curve>,
        /// Length (m) and cross-section (m²).
        length: f64,
        /// Cross-section (m²).
        area: f64,
    },
    /// A capacitor (F).
    Capacitor(f64),
    /// An inductor (H).
    Inductor(f64),
    /// An ideal voltage source: v_a − v_b = volts.
    VoltageSource(f64),
    /// An ideal current source: its current flows through it from `a` to `b` (out of the
    /// circuit at node `a`, back into it at node `b`).
    CurrentSource(f64),
    /// A switch: a contact of [`CONTACT`] Ω when closed, nothing when open.
    Switch,
    /// A fuse: its element melts once ∫i²·dt reaches `melting` (A²·s, the pre-arcing I²t of
    /// its datasheet).
    Fuse {
        /// Pre-arcing I²t (A²·s).
        melting: f64,
    },
    /// A thermal-magnetic circuit breaker rated `rated` A: its bimetal trips as
    /// t = `thermal` / ((I/I_n)² − 1) (an inverse-time curve), its magnet at once beyond
    /// `magnetic`·I_n (curve B: 3–5, C: 5–10, D: 10–20).
    Breaker {
        /// Rated current I_n (A).
        rated: f64,
        /// The thermal curve's constant (s): at 2·I_n it trips in thermal/3.
        thermal: f64,
        /// The instantaneous trip, as a multiple of I_n.
        magnetic: f64,
    },
    /// A diode or LED, anode at `a`.
    Diode(Diode),
    /// A battery, positive at `a`.
    Battery(Battery),
    /// A solar module, positive at `a`.
    Solar(SolarModule),
    /// A DC machine (motor or generator), its armature from `a` to `b`, its rotor on a
    /// shaft: load torque set from outside (positive: braking it).
    Machine(DcMachine),
}

/// The resistance of a closed contact (Ω): a switch's, a fuse's or a breaker's.
pub const CONTACT: f64 = 1e-3;

/// What a component remembers between steps.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// Its voltage and current now and one step before (for the integration's history).
    pub v: [f64; 2],
    pub i: [f64; 2],
    /// Whether a switch is closed, a fuse whole, a breaker on.
    pub closed: bool,
    /// A fuse's ∫i²·dt, a breaker's bimetal (fraction of its trip), a battery's available
    /// well's level (0–1), a machine's rotor speed (rad/s).
    pub level: f64,
    /// A battery's RC pair voltage now and a step before, and its bound well's level; a
    /// machine's rotor speed a step before; a solar module's irradiance (W/m²).
    pub extra: [f64; 3],
    /// Heat it gave off since that was last taken (J).
    pub heat: creeer_network::Sum,
    /// A wire's, a module's or a battery's temperature (K), set from outside.
    pub temperature: f64,
    /// Load torque on a machine's rotor (N·m), set from outside.
    pub load: f64,
    /// A machine's rotor on a drivetrain instead of its own: the speed it would end the
    /// coming step with (rad/s) and how much faster per N·m (`creeer_mech`'s response).
    pub shaft: Option<(f64, f64)>,
}

/// A component in a circuit.
#[derive(Clone, Debug)]
pub struct Component {
    /// Its kind and parameters.
    pub kind: Kind,
    /// Its terminals.
    pub a: Node,
    pub b: Node,
    /// Its state.
    pub state: State,
}

/// A machine's rotor on a drivetrain, for one step: its speed now (rad/s), the speed it
/// would end the step with (rad/s), and how much faster per N·m of the machine's torque
/// (rad/s per N·m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shaft {
    pub speed: f64,
    pub free: f64,
    pub mobility: f64,
}
