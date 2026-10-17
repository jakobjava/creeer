//! Logic on machines (F6.7, ADR-0015): a logic network's sensors read the domains, and its
//! actuators set them, once a tick.
//!
//! Each tick: every sensor takes its reading of the plant as the last steps left it, the
//! network ticks, and every actuator's setting goes to the plant for the steps to come. A
//! controller therefore acts on what it measured a tick before, as a real one does.

use creeer_electric::{Circuit, ComponentId};
use creeer_fluids::pipes::{LinkKind, Network as Pipes};
use creeer_logic::{BlockId, Network};
use creeer_mech::{Drivetrain, Node};

/// The domains a network reads and sets (any may be absent).
#[derive(Debug, Default)]
pub struct Plant<'a> {
    pub pipes: Option<&'a mut Pipes>,
    pub circuit: Option<&'a mut Circuit>,
    pub drivetrains: Option<&'a mut [Drivetrain]>,
}

/// What a sensor reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Probe {
    /// A pipe node's head (m).
    Head(usize),
    /// A pipe link's flow (m³/s).
    Flow(usize),
    /// A drivetrain part's speed (rad/s): the drivetrain and the part.
    Speed(usize, Node),
    /// A circuit node's voltage (V).
    Voltage(creeer_electric::Node),
    /// A component's current (A).
    Current(ComponentId),
}

/// What an actuator sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Setting {
    /// A pump's speed, a fraction of its rated speed (0–1.2).
    PumpSpeed(usize),
    /// A valve's opening (0–1).
    ValveOpening(usize),
    /// A source's voltage or current (V or A), between the given bounds.
    Source(ComponentId, f64, f64),
}

/// A logic network wired to a plant.
#[derive(Debug, Default)]
pub struct Controls {
    pub logic: Network,
    sensors: Vec<(BlockId, Probe)>,
    actuators: Vec<(BlockId, Setting)>,
}

impl Controls {
    /// Controls running `logic`.
    pub fn new(logic: Network) -> Self {
        Self {
            logic,
            ..Self::default()
        }
    }

    /// Feeds sensor block `sensor` with `probe`'s reading every tick.
    pub fn sense(&mut self, sensor: BlockId, probe: Probe) {
        self.sensors.push((sensor, probe));
    }

    /// Sends output `actuator`'s setting (its first output) to `setting` every tick.
    pub fn actuate(&mut self, actuator: BlockId, setting: Setting) {
        self.actuators.push((actuator, setting));
    }

    /// One tick of `dt` seconds: readings in, the network's tick, settings out.
    ///
    /// # Panics
    /// If a probe or a setting names a domain the plant lacks.
    pub fn tick(&mut self, plant: &mut Plant<'_>, dt: f64) {
        for &(sensor, probe) in &self.sensors {
            self.logic.set_sensor(sensor, read(plant, probe));
        }
        self.logic.tick(dt);
        for &(actuator, setting) in &self.actuators {
            let value = self.logic.value((actuator, 0)).number();
            apply(plant, setting, value);
        }
    }
}

fn read(plant: &Plant<'_>, probe: Probe) -> f64 {
    let pipes = || plant.pipes.as_deref().expect("a pipe network");
    let circuit = || plant.circuit.as_deref().expect("a circuit");
    match probe {
        Probe::Head(n) => pipes().nodes[n].head,
        Probe::Flow(l) => pipes().links[l].flow,
        Probe::Speed(d, node) => plant.drivetrains.as_deref().expect("drivetrains")[d].speed(node),
        Probe::Voltage(n) => circuit().voltage(n),
        Probe::Current(c) => circuit().current(c),
    }
}

fn apply(plant: &mut Plant<'_>, setting: Setting, value: f64) {
    // A NaN setting (a script's 0/0) leaves the plant as it was.
    if value.is_nan() {
        return;
    }
    match setting {
        Setting::PumpSpeed(l) => {
            let pipes = plant.pipes.as_deref_mut().expect("a pipe network");
            if let LinkKind::Pump { speed, .. } = &mut pipes.links[l].kind {
                *speed = value.clamp(0.0, 1.2);
            }
        }
        Setting::ValveOpening(l) => {
            let pipes = plant.pipes.as_deref_mut().expect("a pipe network");
            if let LinkKind::Valve { opening, .. } = &mut pipes.links[l].kind {
                *opening = value.clamp(0.0, 1.0);
            }
        }
        Setting::Source(c, low, high) => {
            let circuit = plant.circuit.as_deref_mut().expect("a circuit");
            circuit.set_source(c, value.clamp(low, high));
        }
    }
}

/// A PI controller as a microcontroller's script: input 1 the measurement, input 2 the
/// setpoint, output 1 the setting within [low, high]; its integral (from `start`, the setting
/// expected to hold the plant) in `memory`, kept by a save, and held while the setting is at a
/// bound and the error would push it further.
pub fn pi_script(kp: f64, ki: f64, (low, high): (f64, f64), start: f64) -> String {
    format!(
        "memory.integral = memory.integral or {start}
function tick(dt)
  local err = input[2] - input[1]
  local u = {kp} * err + memory.integral
  if (u < {high} or err < 0) and (u > {low} or err > 0) then
    memory.integral = memory.integral + {ki} * err * dt
  end
  output[1] = math.max({low}, math.min({high}, {kp} * err + memory.integral))
end"
    )
}
