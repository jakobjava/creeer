//! Lua controllers on machines: a PI script holds a tank's level with its pump while the taps
//! open wider, and a motor's speed with its supply's voltage while a load comes on.

use crate::controls::{Controls, Plant, Probe, Setting, pi_script as pi};
use crate::drive::{self, MachineLink};
use creeer_electric::{Circuit, DcMachine, GROUND};
use creeer_fluids::pipes::{LinkKind, Liquid, Network as Pipes, NodeKind};
use creeer_logic::{Kind, Limits, Network, Script, Signal};
use creeer_math::glam::DVec3;
use creeer_mech::{Drivetrain, Element};

const DT: f64 = 1.0 / 60.0;

/// A network of a microcontroller running `source` on a measurement and a setpoint, and an
/// actuator on its output: (network, measurement's sensor, actuator).
fn controller(
    source: &str,
    setpoint: f64,
) -> (Network, creeer_logic::BlockId, creeer_logic::BlockId) {
    let mut net = Network::new();
    let measured = net.add(Kind::Sensor);
    let target = net.add(Kind::Constant(Signal::Number(setpoint)));
    let script = net.add_script(Script::new(source, 2, 1, Limits::default(), 1));
    let actuator = net.add(Kind::Actuator);
    net.wire((measured, 0), script, 0);
    net.wire((target, 0), script, 1);
    net.wire((script, 0), actuator, 0);
    (net, measured, actuator)
}

/// A pump lifts well water to a 2 m² tank (floor 20 m up) that drains through a valve to the
/// taps. Held at 2.5 m from 2 m, the level settles within a centimetre in two minutes; when the
/// taps open from 30 % to 70 %, the pump speeds up and the level comes back within a
/// centimetre.
#[test]
fn a_script_holds_a_tanks_level() {
    let mut pipes = Pipes::new(Liquid::WATER_20C, 9.806_65);
    let well = pipes.node(-5.0, NodeKind::Reservoir, -5.0);
    let riser = pipes.node(0.0, NodeKind::Junction { demand: 0.0 }, 0.0);
    let tank = pipes.node(
        20.0,
        NodeKind::Tank {
            area: 2.0,
            floor: 20.0,
        },
        22.0,
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
    let (net, measured, actuator) = controller(&pi(4.0, 0.2, (0.0, 1.2), 0.8), 22.5);
    let mut controls = Controls::new(net);
    controls.sense(measured, Probe::Head(tank));
    controls.actuate(actuator, Setting::PumpSpeed(pump));
    let mut run = |pipes: &mut Pipes, seconds: usize| {
        let mut worst: f64 = 0.0;
        for k in 0..seconds * 60 {
            controls.tick(
                &mut Plant {
                    pipes: Some(&mut *pipes),
                    ..Plant::default()
                },
                DT,
            );
            pipes.tick(DT);
            if k >= (seconds - 10) * 60 {
                worst = worst.max((pipes.nodes[tank].head - 22.5).abs());
            }
        }
        worst
    };
    let settled = run(&mut pipes, 120);
    assert!(settled < 0.01, "level {} m off", settled);
    if let LinkKind::Valve { opening, .. } = &mut pipes.links[valve].kind {
        *opening = 0.7;
    }
    let again = run(&mut pipes, 120);
    assert!(again < 0.01, "level {} m off after the taps opened", again);
    let LinkKind::Pump { speed, .. } = pipes.links[pump].kind else {
        unreachable!("a pump")
    };
    assert!(speed > 0.0 && speed < 1.2, "pump at {speed}");
}

/// A scooter motor through a 10:1 gearbox on a wheel, its supply set by a PI script between 0
/// and 24 V to hold 150 rad/s. It gets there within 0.5 % in three seconds, and back within
/// 0.5 % two seconds after a load of 10 N·m comes on the wheel.
#[test]
fn a_script_holds_a_motors_speed() {
    let mut c = Circuit::new();
    let plus = c.node();
    let supply = c.voltage_source(plus, GROUND, 0.0);
    let machine = c.machine(plus, GROUND, DcMachine::SCOOTER_24V);
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
    let mut trains = vec![d];
    let links = [MachineLink {
        machine,
        drivetrain: 0,
        node: rotor,
    }];
    let (net, measured, actuator) = controller(&pi(0.05, 1.0, (0.0, 24.0), 0.0), 150.0);
    let mut controls = Controls::new(net);
    controls.sense(measured, Probe::Speed(0, rotor));
    controls.actuate(actuator, Setting::Source(supply, 0.0, 24.0));
    let mut run = |c: &mut Circuit, trains: &mut Vec<Drivetrain>, seconds: usize| {
        for _ in 0..seconds * 60 {
            let mut plant = Plant {
                circuit: Some(&mut *c),
                drivetrains: Some(trains.as_mut_slice()),
                ..Plant::default()
            };
            controls.tick(&mut plant, DT);
            drive::step(c, trains, &links, DT, &[DVec3::ZERO]);
        }
        trains[0].speed(rotor)
    };
    let w = run(&mut c, &mut trains, 3);
    assert!((w / 150.0 - 1.0).abs() < 5e-3, "{w} rad/s");
    // The wheel turns backwards (the gearbox reverses): the load acts forwards.
    trains[0].set_load(wheel, 10.0);
    let w = run(&mut c, &mut trains, 2);
    assert!((w / 150.0 - 1.0).abs() < 5e-3, "{w} rad/s under the load");
}
