//! The powerhouse as built: its sizes, and the couplings between its domains.

use super::{Powerhouse, Stores, Thermals};
use crate::controls::{Controls, Probe, Setting, pi_script};
use crate::drive::MachineLink;
use crate::electrochem::CellLink;
use crate::hydro::TurbineLink;
use crate::mount::Mount;
use creeer_chem::System;
use creeer_chem::electrolysis::Electrochemical;
use creeer_chem::kinetics::Reaction;
use creeer_chem::reactor::{Holding, Reactor};
use creeer_electric::{Circuit, DcMachine, Diode, Electrolyzer, GROUND};
use creeer_fluids::pipes::{LinkKind, Liquid, Network as Pipes, NodeKind};
use creeer_logic::{Kind, Limits, Network, Script};
use creeer_materials::MaterialTable;
use creeer_materials::species::SpeciesTable;
use creeer_math::glam::DVec3;
use creeer_mech::{Drivetrain, Element};
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};
use creeer_thermal::{Enthalpy, Link, Network as Heat};
use std::sync::Arc;

use super::{DT, G};

/// The shaft speed the controller holds (rad/s, 1,430 rpm).
pub const SPEED: f64 = 150.0;

/// The generator: 0.16 V·s/rad (24 V at 150 rad/s), 20 mΩ, 1 mH; its rotor's inertia and
/// friction are the drivetrain's.
pub const GENERATOR: DcMachine = DcMachine {
    constant: 0.16,
    resistance: 0.02,
    inductance: 1.0e-3,
    inertia: 0.0,
    friction: 0.0,
};

/// A 150 A rectifier between the generator and the stack, so the stack cannot drive it as a
/// motor (a fuel cell, with no hydrogen yet): 0.9 V at 140 A.
const BLOCKING: Diode = Diode {
    saturation: 1.0e-7,
    emission: 1.2,
    series: 2.0e-3,
    light: 0.0,
};

/// The upper tank's and the lower basin's areas (m²) and floors (m), and the water in them (m).
const TANK: (f64, f64, f64) = (30.0, 25.0, 5.0);
const BASIN: (f64, f64, f64) = (30.0, 0.0, 1.0);

fn pipes() -> (Pipes, [usize; 4]) {
    let mut p = Pipes::new(Liquid::WATER_20C, G);
    let tank = |(area, floor, depth): (f64, f64, f64)| {
        (floor, NodeKind::Tank { area, floor }, floor + depth)
    };
    let (z, kind, head) = tank(TANK);
    let upper = p.node(z, kind, head);
    let foot = p.node(0.5, NodeKind::Junction { demand: 0.0 }, 0.5);
    let inlet = p.node(0.5, NodeKind::Junction { demand: 0.0 }, 0.5);
    let (z, kind, head) = tank(BASIN);
    let lower = p.node(z, kind, head);
    // An 80 m steel penstock of 12 cm, its bends and entry.
    p.link(
        upper,
        foot,
        LinkKind::Pipe {
            length: 80.0,
            diameter: 0.12,
            roughness: 5e-5,
            minor: 1.5,
            check: false,
        },
    );
    let valve = p.link(
        foot,
        inlet,
        LinkKind::Valve {
            diameter: 0.05,
            open: 0.5,
            opening: 0.5,
        },
    );
    // A small reaction runner: 10 cm at its outlet, best near 150 rad/s at 12 L/s.
    let turbine = p.link(
        inlet,
        lower,
        LinkKind::Turbine {
            swirl: 245.0,
            radius: 0.1,
            loss: 1.0e4,
            speed: 0.0,
        },
    );
    (p, [upper, lower, valve, turbine])
}

/// Runner and generator rotor on a vertical shaft, each in its bearing.
fn drivetrain() -> (Drivetrain, [usize; 3], creeer_mech::Node, creeer_mech::Node) {
    let mut d = Drivetrain::new();
    let runner = d.part(0.6, DVec3::Z);
    let rotor = d.part(0.15, DVec3::Z);
    let shaft = d.add(Element::Shaft {
        a: runner,
        b: rotor,
        stiffness: 2.0e4,
        damping: 2.0,
    });
    let lower = d.add(Element::Bearing {
        a: runner,
        viscous: 1.0e-3,
        coulomb: 0.2,
    });
    let upper = d.add(Element::Bearing {
        a: rotor,
        viscous: 1.0e-3,
        coulomb: 0.1,
    });
    (d, [shaft, lower, upper], runner, rotor)
}

/// Twelve alkaline cells of 300 cm² splitting water into a reactor at 1 bar.
fn electrolysis() -> (System, Reactor, Reaction) {
    let table = SpeciesTable::reference();
    let sys = System::new(&table, &["H2O(L)", "H2", "O2", "N2"]).expect("species");
    let mut moles = vec![0.0; sys.species.len()];
    moles[sys.index("H2O(L)").expect("water")] = 500.0;
    moles[sys.index("N2").expect("nitrogen")] = 0.1;
    let mut reactor = Reactor::new(&sys, moles, 293.15, Holding::Pressure(1.0e5), Vec::new());
    reactor.ua = 20.0;
    reactor.surroundings = 293.15;
    let split = Reaction::new(
        &sys,
        &[("H2O(L)", 1.0)],
        &[("H2", 1.0), ("O2", 0.5)],
        0.0,
        0.0,
        0.0,
    )
    .expect("water's splitting");
    (sys, reactor, split)
}

/// The generator's iron, the frame round the bearings, the stack's coolant, the air.
fn thermals() -> (Heat, Thermals) {
    let steel = MaterialTable::reference()
        .get("steel_s235")
        .expect("steel")
        .clone();
    let steel = Arc::new(Enthalpy::new(&steel.specific_heat, steel.melting));
    let mut heat = Heat::new(G);
    let generator = heat.add_mass(60.0, steel.clone(), 293.15);
    let frame = heat.add_mass(300.0, steel, 293.15);
    let coolant = heat.add_mass(200.0, Arc::new(Enthalpy::constant(4186.0)), 293.15);
    let air = heat.add_fixed(293.15);
    for (node, conductance) in [(generator, 15.0), (frame, 25.0), (coolant, 50.0)] {
        heat.link(Link::Conductance {
            nodes: (node, air),
            conductance,
        });
    }
    let nodes = Thermals {
        generator,
        frame,
        coolant,
    };
    (heat, nodes)
}

/// A 3 × 2 × 1 m, 2 t platform, free (gravity off: on a frictionless turntable, every
/// reaction torque turns it).
fn platform() -> (World, creeer_math::glam::DMat3) {
    let shape = Shape::Box {
        half: DVec3::new(1.5, 1.0, 0.5),
    };
    let mass = 2000.0;
    let inertia = shape.inertia(mass);
    let mut world = World::new(DT);
    let mut body = Body::rigid(shape, mass, DVec3::ZERO, DT);
    body.structure.gravity = DVec3::ZERO;
    world.add(body);
    (world, inertia)
}

impl Powerhouse {
    /// The powerhouse at rest, its valve half open, its controller holding [`SPEED`].
    pub fn new() -> Self {
        let (pipes, [upper, lower, valve, turbine]) = pipes();
        let (d, frictions, runner, rotor) = drivetrain();
        let mut circuit = Circuit::new();
        let plus = circuit.node();
        let generator = circuit.machine(plus, GROUND, GENERATOR);
        let stack_plus = circuit.node();
        let rectifier = circuit.diode(plus, stack_plus, BLOCKING);
        let cells = Electrolyzer {
            cells: 12.0,
            ..Electrolyzer::ALKALINE_CELL
        };
        let stack = circuit.electrolyzer(stack_plus, GROUND, cells);
        let (sys, reactor, split) = electrolysis();
        let (heat, thermals) = thermals();
        let (world, platform_inertia) = platform();
        let mut logic = Network::new();
        let measured = logic.add(Kind::Sensor);
        let setpoint = logic.add(Kind::Sensor);
        let source = pi_script(0.01, 0.02, (0.05, 1.0), 0.5);
        let script = logic.add_script(Script::new(&source, 2, 1, Limits::default(), 1));
        let actuator = logic.add(Kind::Actuator);
        logic.wire((measured, 0), script, 0);
        logic.wire((setpoint, 0), script, 1);
        logic.wire((script, 0), actuator, 0);
        let mut controls = Controls::new(logic);
        controls.sense(measured, Probe::Speed(0, runner));
        controls.actuate(actuator, Setting::ValveOpening(valve));
        let mut house = Self {
            pipes,
            upper,
            lower,
            valve,
            turbine: [TurbineLink {
                turbine,
                drivetrain: 0,
                node: runner,
            }],
            trains: vec![d],
            runner,
            frictions,
            rectifier,
            machine: [MachineLink {
                machine: generator,
                drivetrain: 0,
                node: rotor,
            }],
            circuit,
            cells: [CellLink {
                cell: stack,
                cells: 12.0,
                electrochemical: Electrochemical {
                    reaction: split,
                    electrons: 2.0,
                },
            }],
            sys,
            reactor,
            heat,
            thermals,
            controls,
            setpoint,
            speed: SPEED,
            world,
            platform_inertia,
            mount: Mount::new(0, 0),
            time: 0.0,
            start: Stores::default(),
            moved: super::Moved::default(),
        };
        house.start = house.stores();
        house
    }
}
