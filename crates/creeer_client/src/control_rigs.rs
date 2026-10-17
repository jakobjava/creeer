//! The control panel's rigs (`control_panel.rs`): a tank on a pump and the taps, a motor on
//! its supply and a wheel, and the PI script each starts with.

use creeer_electric::{Circuit, DcMachine, GROUND};
use creeer_fluids::pipes::{LinkKind, Liquid, Network as Pipes, NodeKind};
use creeer_math::glam::DVec3;
use creeer_mech::{Drivetrain, Element, Node};
use creeer_sim::drive::MachineLink;

/// The tank's floor (m above the pump).
pub const FLOOR: f64 = 20.0;

/// The script a rig starts with: a PI controller, its integral in `memory`.
const PI: &str = "-- input[1]: measured, input[2]: setpoint; output[1]: setting
memory.integral = memory.integral or START
function tick(dt)
  local err = input[2] - input[1]
  local u = KP * err + memory.integral
  if (u < HIGH or err < 0) and (u > LOW or err > 0) then
    memory.integral = memory.integral + KI * err * dt
  end
  output[1] = math.max(LOW, math.min(HIGH, KP * err + memory.integral))
end";

/// A rig's machine.
pub enum Machine {
    Tank {
        pipes: Box<Pipes>,
        tank: usize,
        valve: usize,
    },
    Motor {
        circuit: Box<Circuit>,
        trains: Vec<Drivetrain>,
        link: MachineLink,
        rotor: Node,
        wheel: Node,
    },
}

/// The PI script with its gains and bounds.
pub fn pi(kp: f64, ki: f64, (low, high): (f64, f64), start: f64) -> String {
    PI.replace("KP", &kp.to_string())
        .replace("KI", &ki.to_string())
        .replace("LOW", &low.to_string())
        .replace("HIGH", &high.to_string())
        .replace("START", &start.to_string())
}

pub fn tank() -> (Machine, usize) {
    let mut pipes = Pipes::new(Liquid::WATER_20C, 9.806_65);
    let well = pipes.node(-5.0, NodeKind::Reservoir, -5.0);
    let riser = pipes.node(0.0, NodeKind::Junction { demand: 0.0 }, 0.0);
    let tank = pipes.node(
        FLOOR,
        NodeKind::Tank {
            area: 2.0,
            floor: FLOOR,
        },
        FLOOR + 2.0,
    );
    let down = pipes.node(0.0, NodeKind::Junction { demand: 0.0 }, 0.0);
    let taps = pipes.node(0.0, NodeKind::Reservoir, 0.0);
    let pump = pipes.link(
        well,
        riser,
        LinkKind::Pump {
            shutoff: 45.0,
            resistance: 1.5e4,
            speed: 0.0,
            best_flow: 0.03,
            best_efficiency: 0.75,
        },
    );
    let pipe = |length| LinkKind::Pipe {
        length,
        diameter: 0.1,
        roughness: 5e-5,
        minor: 2.0,
        check: false,
    };
    pipes.link(riser, tank, pipe(80.0));
    pipes.link(tank, down, pipe(300.0));
    let valve = pipes.link(
        down,
        taps,
        LinkKind::Valve {
            diameter: 0.05,
            open: 0.2,
            opening: 0.3,
        },
    );
    (
        Machine::Tank {
            pipes: Box::new(pipes),
            tank,
            valve,
        },
        pump,
    )
}

pub fn motor() -> (Machine, creeer_electric::ComponentId) {
    let mut circuit = Circuit::new();
    let plus = circuit.node();
    let supply = circuit.voltage_source(plus, GROUND, 0.0);
    let machine = circuit.machine(plus, GROUND, DcMachine::SCOOTER_24V);
    let mut d = Drivetrain::new();
    let rotor = d.part(DcMachine::SCOOTER_24V.inertia, DVec3::Z);
    let wheel = d.part(0.5, DVec3::Z);
    d.add(Element::Gear {
        a: rotor,
        b: wheel,
        ratio: -0.1,
        efficiency: 0.95,
        backlash: 0.0,
    });
    d.add(Element::Bearing {
        a: wheel,
        viscous: 0.05,
        coulomb: 0.0,
    });
    let link = MachineLink {
        machine,
        drivetrain: 0,
        node: rotor,
    };
    (
        Machine::Motor {
            circuit: Box::new(circuit),
            trains: vec![d],
            link,
            rotor,
            wheel,
        },
        supply,
    )
}
