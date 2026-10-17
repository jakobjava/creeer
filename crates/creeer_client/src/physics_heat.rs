//! F6.2 heat scenes: blocks cooling in air and in a vacuum, a radiator panel turning in
//! sunlight in orbit, a loaded steel frame in a fire.
//!
//! Heat is time-lapsed in them (the factor is shown): a block cools for half an hour, a
//! fire takes a quarter of one to weaken steel. The motion is not: what heat does to
//! structures is slow beside how they move, so the frame sags as it would, only sooner.

use creeer_materials::MaterialTable;
use creeer_math::glam::{DQuat, DVec3};
use creeer_physics::structure::Section;
use creeer_physics::structure::section::STEEL_S235;
use creeer_physics::structure::shapes::{Base, space_frame};
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};
use creeer_sim::heat::MemberHeat;
use creeer_thermal::environment::{DEEP_SPACE, SOLAR_CONSTANT, iso_834};
use creeer_thermal::{Enthalpy, Link, Medium, Network, NodeId, Shape as Surface};
use std::sync::Arc;

/// Ticks per heat step (60 Hz physics, 20 Hz heat).
const EVERY: u32 = 3;
const ATMOSPHERE: f64 = 101_325.0;
const ROOM: f64 = 293.15;

/// A scene's heat.
#[derive(Debug)]
enum Kind {
    /// Two steel blocks, one in air, one in a vacuum: their nodes and bodies.
    Blocks {
        network: Network,
        nodes: [NodeId; 2],
        bodies: [usize; 2],
    },
    /// A panel in orbit: its node and body, its face area, its solar absorptivity, the
    /// heat its electronics give it (W), the direction to the Sun.
    Radiator {
        network: Network,
        node: NodeId,
        body: usize,
        area: f64,
        absorptivity: f64,
        load: f64,
        sun: DVec3,
    },
    /// A frame in a fire.
    Fire {
        heat: Box<MemberHeat>,
        fire: usize,
        body: usize,
    },
}

/// The heat of a scene, time-lapsed by `warp`.
#[derive(Debug)]
pub struct SceneHeat {
    kind: Kind,
    warp: f64,
    /// Heat's own time (s).
    time: f64,
    ticks: u32,
}

impl SceneHeat {
    /// Advances by one physics tick of `dt` seconds (heat steps every [`EVERY`] ticks).
    pub fn tick(&mut self, world: &mut World, dt: f64) {
        self.ticks += 1;
        if !self.ticks.is_multiple_of(EVERY) {
            return;
        }
        let step = f64::from(EVERY) * dt * self.warp;
        self.time += step;
        match &mut self.kind {
            Kind::Blocks { network, .. } => {
                network.step(step);
            }
            Kind::Radiator {
                network,
                node,
                body,
                area,
                absorptivity,
                load,
                sun,
            } => {
                // Its two faces, normals ±y of the body: sunlight on whichever faces the Sun.
                let normal = world.bodies[*body].structure.orientation[0] * DVec3::Y;
                let sunlit = *absorptivity * *area * SOLAR_CONSTANT * normal.dot(*sun).abs();
                network.set_source(*node, *load + sunlit);
                network.step(step);
            }
            Kind::Fire { heat, fire, body } => {
                heat.set_fire(*fire, iso_834(self.time));
                let b = &mut world.bodies[*body];
                let (report, _) = heat.step(step, &mut b.structure, Some(&mut b.solver));
                if report.solved > 0 {
                    b.wake();
                }
            }
        }
    }

    /// The temperature (°C) of body `body`, if it is one heat follows as a whole.
    pub fn body_temperature(&self, body: usize) -> Option<f64> {
        match &self.kind {
            Kind::Blocks {
                network,
                nodes,
                bodies,
            } => bodies
                .iter()
                .position(|&b| b == body)
                .map(|k| network.temperature(nodes[k]) - 273.15),
            Kind::Radiator {
                network,
                node,
                body: b,
                ..
            } => (*b == body).then(|| network.temperature(*node) - 273.15),
            Kind::Fire { .. } => None,
        }
    }

    /// The temperature (°C) of each member of body `body`, if heat follows its members.
    pub fn member_temperatures(&self, body: usize, members: usize) -> Option<Vec<f64>> {
        match &self.kind {
            // A member split since the last heat step is followed at the next.
            Kind::Fire { heat, body: b, .. } if *b == body => Some(
                (0..members)
                    .map(|k| {
                        if k < heat.members() {
                            heat.temperature(k) - 273.15
                        } else {
                            f64::NAN
                        }
                    })
                    .collect(),
            ),
            _ => None,
        }
    }

    /// The fire's gas temperature (°C), if there is a fire.
    pub fn fire(&self) -> Option<f64> {
        matches!(self.kind, Kind::Fire { .. }).then(|| iso_834(self.time) - 273.15)
    }

    /// A line for the scene's window.
    pub fn label(&self) -> String {
        let minutes = self.time / 60.0;
        let what = match &self.kind {
            Kind::Blocks { network, nodes, .. } => format!(
                "in air {:.0} °C · in vacuum {:.0} °C",
                network.temperature(nodes[0]) - 273.15,
                network.temperature(nodes[1]) - 273.15
            ),
            Kind::Radiator { network, node, .. } => {
                format!("panel {:.0} °C", network.temperature(*node) - 273.15)
            }
            Kind::Fire { heat, .. } => {
                let hottest = (0..heat.members())
                    .map(|k| heat.temperature(k))
                    .fold(0.0, f64::max);
                format!(
                    "fire {:.0} °C (ISO 834) · hottest member {:.0} °C",
                    iso_834(self.time) - 273.15,
                    hottest - 273.15
                )
            }
        };
        format!("heat ×{:.0}: {minutes:.1} min · {what}", self.warp)
    }
}

fn steel() -> creeer_materials::Material {
    MaterialTable::reference()
        .get("steel_s235")
        .expect("in the reference table")
        .clone()
}

/// Two 20 cm steel blocks at 900 °C on firebrick, one in air, one in a vacuum chamber (its
/// walls at room temperature): radiation cools both alike while they glow; below a few
/// hundred degrees the air's convection is what tells them apart.
pub fn blocks(world: &mut World) -> SceneHeat {
    let dt = world.dt();
    let steel = steel();
    let enthalpy = Arc::new(Enthalpy::new(&steel.specific_heat, steel.melting));
    let mut network = Network::new(9.806_65);
    let (air, room) = (network.add_fixed(ROOM), network.add_fixed(ROOM));
    let side = 0.2;
    let mass = steel.density * side * side * side;
    let mut nodes = [0; 2];
    let mut bodies = [0; 2];
    for (k, x) in [-1.0, 1.0].into_iter().enumerate() {
        crate::physics_sandbox::wall(world, DVec3::new(x, 0.25, 0.0), DVec3::new(0.3, 0.25, 0.3));
        let block = Body::rigid(
            Shape::Box {
                half: DVec3::splat(0.5 * side),
            },
            mass,
            DVec3::new(x, 0.5 + 0.5 * side, 0.0),
            dt,
        );
        bodies[k] = world.add(block);
        let node = network.add_mass(mass, enthalpy.clone(), 1173.15);
        // Five faces show; the sixth lies on firebrick (k 0.2 W/(m·K), 5 cm to its cool side).
        network.link(Link::Radiation {
            nodes: (node, room),
            factor: steel.emissivity * 5.0 * side * side,
        });
        network.link(Link::Conductance {
            nodes: (node, room),
            conductance: 0.2 * side * side / 0.05,
        });
        if k == 0 {
            let (sides, top) = (4.0 * side * side, side * side);
            network.link(Link::Convection {
                nodes: (node, air),
                medium: Medium::Air,
                area: sides,
                shape: Surface::VerticalPlate(side),
                speed: 0.0,
                pressure: ATMOSPHERE,
            });
            network.link(Link::Convection {
                nodes: (node, air),
                medium: Medium::Air,
                area: top,
                shape: Surface::PlateUp(0.25 * side),
                speed: 0.0,
                pressure: ATMOSPHERE,
            });
        }
        nodes[k] = node;
    }
    SceneHeat {
        kind: Kind::Blocks {
            network,
            nodes,
            bodies,
        },
        warp: 60.0,
        time: 0.0,
        ticks: 0,
    }
}

/// A 1 × 2 m aluminium radiator panel, 1.5 mm thick and painted white (α 0.25, ε 0.85), in
/// orbit (nothing around it but deep space and the Sun), turning about its long axis once
/// in two minutes and taking 300 W from the electronics it cools.
pub fn radiator(world: &mut World, sun: DVec3) -> SceneHeat {
    let dt = world.dt();
    let aluminium = MaterialTable::reference()
        .get("aluminium_6061_t6")
        .expect("in the table")
        .clone();
    let (x, z, t) = (1.0, 2.0, 0.0015);
    let mass = aluminium.density * x * z * t;
    let mut panel = Body::rigid(
        Shape::Box {
            half: DVec3::new(0.5 * x, 0.5 * t, 0.5 * z),
        },
        mass,
        DVec3::new(0.0, 4.0, 0.0),
        dt,
    );
    panel.structure.gravity = DVec3::ZERO;
    // About its long axis (z, its least inertia: a stable spin).
    panel.structure.angular_velocity[0] = DVec3::Z * (std::f64::consts::TAU / 120.0);
    panel.structure.orientation[0] = DQuat::from_rotation_z(0.3);
    let body = world.add(panel);
    let mut network = Network::new(0.0);
    let space = network.add_fixed(DEEP_SPACE);
    let enthalpy = Arc::new(Enthalpy::new(&aluminium.specific_heat, aluminium.melting));
    let node = network.add_mass(mass, enthalpy, ROOM);
    let (emissivity, area) = (0.85, x * z);
    network.link(Link::Radiation {
        nodes: (node, space),
        factor: emissivity * 2.0 * area,
    });
    let kind = Kind::Radiator {
        network,
        node,
        body,
        area,
        absorptivity: 0.25,
        load: 300.0,
        sun,
    };
    SceneHeat {
        kind,
        warp: 20.0,
        time: 0.0,
        ticks: 0,
    }
}

/// A 4 m steel box frame (welded, no diagonals) of 140 mm tubes on two concrete piers,
/// carrying a 9 t block (a quarter of its plastic capacity), with a
/// fire burning under its middle half (the ISO 834 standard fire). As its members pass
/// 500 °C steel loses strength fast (EN 1993-1-2: half its yield strength at 600 °C, a
/// quarter at 700 °C), and it sags.
pub fn fire(world: &mut World) -> SceneHeat {
    let dt = world.dt();
    let tube = Section::round_tube(0.14, 0.006);
    let mut frame = space_frame([4, 1, 1], 1.0, tube, STEEL_S235, Base::Free);
    frame.translate(DVec3::new(-2.0, 1.5 + tube.extent[0], -0.5));
    let burning: Vec<usize> = frame
        .member_geometry()
        .iter()
        .enumerate()
        .filter(|(_, (a, b, ..))| (0.5 * (frame.position[*a] + frame.position[*b])).x.abs() <= 1.01)
        .map(|(k, _)| k)
        .collect();
    let steel = steel();
    let mut heat = MemberHeat::new(&frame, &steel, frame.gravity);
    heat.in_air(&frame, ROOM, ATMOSPHERE, ROOM);
    let fire = heat.fire(&frame, &burning, iso_834(0.0));
    let body = world.add(Body::structure(frame, dt));
    for x in [-2.3, 2.3] {
        crate::physics_sandbox::wall(world, DVec3::new(x, 0.75, 0.0), DVec3::new(0.4, 0.75, 0.8));
    }
    let top = 2.5 + 2.0 * tube.extent[0];
    world.add(Body::rigid(
        Shape::Box {
            half: DVec3::new(0.6, 0.4, 0.6),
        },
        9000.0,
        DVec3::new(0.0, top + 0.41, 0.0),
        dt,
    ));
    SceneHeat {
        kind: Kind::Fire {
            heat: Box::new(heat),
            fire,
            body,
        },
        warp: 30.0,
        time: 0.0,
        ticks: 0,
    }
}
