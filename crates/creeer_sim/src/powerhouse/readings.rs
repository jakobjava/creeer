//! What a panel shows of the powerhouse.

use super::Powerhouse;
use creeer_fluids::pipes::LinkKind;

/// The powerhouse's state at a glance.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Readings {
    /// The shaft's speed (rad/s) and the valve's opening (0–1).
    pub speed: f64,
    pub valve: f64,
    /// The flow through the turbine (m³/s) and the upper tank's level above its floor (m).
    pub flow: f64,
    pub level: f64,
    /// The turbine's power into the shaft and the stack's electrical power (W).
    pub turbine: f64,
    pub stack: f64,
    /// The stack's current (A) and the hydrogen made (mol).
    pub current: f64,
    pub hydrogen: f64,
    /// Temperatures (K): the generator's iron, the frame, the coolant, the stack.
    pub temperatures: [f64; 4],
    /// The platform's spin about the vertical (rad/s).
    pub spin: f64,
}

impl Powerhouse {
    /// What it shows now.
    pub fn readings(&self) -> Readings {
        let speed = self.trains[0].speed(self.runner);
        let valve = match self.pipes.links[self.valve].kind {
            LinkKind::Valve { opening, .. } => opening,
            _ => f64::NAN,
        };
        let turbine = self.turbine[0].turbine;
        let level = match self.pipes.nodes[self.upper].kind {
            creeer_fluids::pipes::NodeKind::Tank { floor, .. } => {
                self.pipes.nodes[self.upper].head - floor
            }
            _ => f64::NAN,
        };
        let t = self.thermals;
        let hydrogen = self.sys.index("H2").map_or(0.0, |j| self.reactor.moles[j]);
        Readings {
            speed,
            valve,
            flow: self.pipes.links[turbine].flow,
            level,
            turbine: self.pipes.turbine_torque(turbine, speed) * speed,
            stack: self.reactor.power,
            current: self.circuit.current(self.cells[0].cell),
            hydrogen,
            temperatures: [
                self.heat.temperature(t.generator),
                self.heat.temperature(t.frame),
                self.heat.temperature(t.coolant),
                self.reactor.t,
            ],
            spin: self.world.bodies[0].structure.angular_velocity[0].z,
        }
    }
}
