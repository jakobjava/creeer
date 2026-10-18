//! F6.8's powerhouse (ADR-0016): one machine through every domain.
//!
//! Water from an upper tank runs down a penstock, through a valve, and through a turbine into
//! a lower basin (fluids). The turbine turns a generator on a shaft (mechanics); the generator
//! feeds a stack of twelve electrolysis cells (electricity), which split water into hydrogen
//! and oxygen in a reactor (chemistry). The generator's armature, the bearings and the stack
//! warm their iron, frame and coolant, which give their heat to the air (thermal). A Lua
//! script holds the shaft's speed with the valve (logic). All of it stands on a platform free
//! to turn, which takes every reaction torque (structure).
//!
//! Each tick, in order:
//! 1. the controller reads the shaft's speed and sets the valve;
//! 2. the turbine's torque from its curve at the shaft's step (`hydro`); the generator's and
//!    the stack's states into the circuit (`drive`, `electrochem`); the circuit's step;
//! 3. the drivetrain's step; the turbine's work out of the water; the reactor's step;
//! 4. the drivetrain's reaction on the platform, and the world's step (`mount`);
//! 5. the pipes' tick; the heats into the thermal network, and its step.
//!
//! Every joule that crosses from one domain to another is booked on both sides as the same
//! number: [`Powerhouse::books`] adds up where the water's energy went.

mod build;
mod machine;
mod readings;
#[cfg(test)]
mod tests;

pub use build::{GENERATOR, SPEED};
pub use readings::Readings;

use crate::controls::{Controls, Plant};
use crate::drive::{self, MachineLink};
use crate::electrochem::{self, CellLink};
use crate::hydro::{self, TurbineLink};
use crate::mount::Mount;
use creeer_chem::System;
use creeer_chem::reactor::Reactor;
use creeer_electric::Circuit;
use creeer_fluids::pipes::{Network as Pipes, NodeKind};
use creeer_logic::BlockId;
use creeer_math::glam::{DMat3, DVec3};
use creeer_mech::{Drivetrain, Node};
use creeer_physics::world::World;
use creeer_thermal::{Network as Heat, NodeId};

/// The tick (s) and gravity (m/s²).
pub const DT: f64 = 1.0 / 60.0;
pub const G: f64 = 9.806_65;

/// The thermal network's masses.
#[derive(Clone, Copy, Debug)]
pub struct Thermals {
    pub generator: NodeId,
    pub frame: NodeId,
    pub coolant: NodeId,
}

/// What each domain holds (J), and what must not change: the water's volume (m³), the
/// reactor's mass (kg), the angular momentum of the platform and the shaft (kg·m²/s).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stores {
    pub water: f64,
    pub shaft: f64,
    pub circuit: f64,
    pub reactor: f64,
    pub heat: f64,
    pub volume: f64,
    pub mass: f64,
    pub momentum: DVec3,
}

/// The joules that crossed between domains so far, each as both sides booked it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Moved {
    /// The turbine's work: out of the water, into the shaft.
    pub turbine: f64,
    /// The stack's electrical energy (its reaction's work and its heat): out of the circuit,
    /// into the reactor.
    pub stack: f64,
    /// Heat from the generator's armature, the bearings and shaft, the reactor.
    pub armature_heat: f64,
    pub friction_heat: f64,
    pub reactor_heat: f64,
}

/// The powerhouse.
pub struct Powerhouse {
    pub pipes: Pipes,
    pub upper: usize,
    pub lower: usize,
    pub valve: usize,
    turbine: [TurbineLink; 1],
    pub trains: Vec<Drivetrain>,
    pub runner: Node,
    /// The drivetrain's elements that lose energy (their heat to the frame).
    frictions: [usize; 3],
    machine: [MachineLink; 1],
    /// The rectifier before the stack (its heat to the generator's iron).
    rectifier: creeer_electric::ComponentId,
    pub circuit: Circuit,
    cells: [CellLink; 1],
    pub sys: System,
    pub reactor: Reactor,
    pub heat: Heat,
    pub thermals: Thermals,
    pub controls: Controls,
    setpoint: BlockId,
    /// The shaft speed the controller holds (rad/s).
    pub speed: f64,
    pub world: World,
    platform_inertia: DMat3,
    pub mount: Mount,
    pub time: f64,
    pub start: Stores,
    pub moved: Moved,
    /// Heat given to the air from outside the thermal network's books (abstract mode's:
    /// `machine.rs`).
    pub air: f64,
}

impl std::fmt::Debug for Powerhouse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Powerhouse")
            .field("time", &self.time)
            .finish_non_exhaustive()
    }
}

impl Default for Powerhouse {
    fn default() -> Self {
        Self::new()
    }
}

impl Powerhouse {
    /// One tick of [`DT`].
    pub fn tick(&mut self) {
        let h = DT;
        self.controls.logic.set_sensor(self.setpoint, self.speed);
        let mut plant = Plant {
            pipes: Some(&mut self.pipes),
            circuit: Some(&mut self.circuit),
            drivetrains: Some(&mut self.trains),
        };
        self.controls.tick(&mut plant, h);
        let spin = self.mount.spin(&self.world, &self.trains[0]);
        let started = hydro::prepare(&self.pipes, &mut self.trains, &self.turbine, h);
        drive::prepare(&mut self.circuit, &mut self.trains, &self.machine, h);
        let emf = electrochem::prepare(&mut self.circuit, &self.reactor, &self.sys, &self.cells);
        self.circuit.step(h);
        drive::finish(&self.circuit, &mut self.trains, &self.machine, h, &[spin]);
        let works = hydro::finish(&mut self.pipes, &self.trains, &self.turbine, &started, h);
        self.moved.turbine += works.iter().sum::<f64>();
        let (_, _, out, into) = self.reactor.ledger().totals();
        electrochem::finish(
            &mut self.circuit,
            &mut self.reactor,
            &self.sys,
            &self.cells,
            &emf,
            h,
        );
        self.mount.push(&mut self.world, &self.trains[0]);
        self.world.step();
        self.mount.settle(&mut self.world, &self.trains[0], h);
        self.pipes.tick(h);
        self.warm(h, out, into);
        self.time += h;
    }

    /// The heats into the thermal network and its step: the armature's, the drivetrain's
    /// friction, and what the reactor gave its coolant (its ledger's outflow since `out`, less
    /// what came in since `into` beyond the stack's electrical energy). The reactor and the
    /// stack then see their coolant's temperature.
    fn warm(&mut self, h: f64, out: f64, into: f64) {
        let t = self.thermals;
        let armature = self.circuit.take_heat(self.machine[0].machine)
            + self.circuit.take_heat(self.rectifier);
        let friction: f64 = self
            .frictions
            .iter()
            .map(|&e| self.trains[0].take_heat(e))
            .sum();
        let (_, _, out_now, into_now) = self.reactor.ledger().totals();
        let electric = self.reactor.power * h;
        self.moved.stack += electric;
        let reactor = (out_now - out) - ((into_now - into) - electric);
        self.heat.receive(t.generator, armature);
        self.heat.receive(t.frame, friction);
        self.heat.receive(t.coolant, reactor);
        self.heat.step(h);
        self.reactor.surroundings = self.heat.temperature(t.coolant);
        self.moved.armature_heat += armature;
        self.moved.friction_heat += friction;
        self.moved.reactor_heat += reactor;
    }

    /// What each domain holds now.
    pub fn stores(&self) -> Stores {
        let volume = [self.upper, self.lower]
            .iter()
            .map(|&n| match self.pipes.nodes[n].kind {
                NodeKind::Tank { area, floor } => area * (self.pipes.nodes[n].head - floor),
                _ => 0.0,
            })
            .sum();
        let s = &self.world.bodies[0].structure;
        let turn = DMat3::from_quat(s.orientation[0]);
        let body = turn * self.platform_inertia * turn.transpose() * s.angular_velocity[0];
        // The mount's correction still owed, and the half of its last torque's impulse the
        // body's Newmark step carries into the next (`mount.rs`).
        let owed = self.mount.owed(DT) + self.mount.given() * (0.5 * DT);
        Stores {
            water: self.pipes.stored(),
            shaft: self.trains[0].stored(),
            circuit: self.circuit.stored(),
            reactor: self.reactor.stored(&self.sys),
            heat: self.heat.stored(),
            volume,
            mass: self.sys.mass(&self.reactor.moles),
            momentum: body + turn * self.trains[0].momentum() + owed,
        }
    }

    /// Where the water's energy went since it was built (J): (what the water gave, what it
    /// went to: the water's own friction, what the shaft and the circuit hold, what the reactor
    /// holds (hydrogen's chemical energy and its warmth), the heat the thermal network holds
    /// and gave the air, and losses no network took up).
    pub fn books(&self) -> (f64, [f64; 6]) {
        let now = self.stores();
        let (_, water_friction, _, _) = self.pipes.ledger().totals();
        let (supplied, to_air, _, _) = self.heat.ledger().totals();
        let (_, shaft_loss, _, _) = self.trains[0].ledger().totals();
        let (_, circuit_loss, cell_work, _) = self.circuit.ledger().totals();
        let unrouted = (shaft_loss - self.moved.friction_heat)
            + (circuit_loss - self.moved.armature_heat - (self.moved.stack - cell_work));
        let gave = self.start.water - now.water;
        (
            gave,
            [
                water_friction,
                now.shaft - self.start.shaft + now.circuit - self.start.circuit,
                now.reactor - self.start.reactor,
                now.heat - self.start.heat,
                to_air - supplied,
                unrouted,
            ],
        )
    }
}
