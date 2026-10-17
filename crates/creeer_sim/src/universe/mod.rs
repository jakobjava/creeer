//! The whole simulated universe (ADR-0007): the bodies of a generated solar system, the
//! vessels, and the physics bubble around the focus, advanced tick by tick with time warp.
//!
//! - **Time warp** up to [`MAX_PHYSICS_WARP`] runs the bubble's physics several times per
//!   tick. Beyond it every vessel is packed (on rails or landed) and only orbits advance,
//!   up to [`MAX_WARP`].
//! - **Frames** (see [`bubble`]): the bubble turns with a body while the focus is within that
//!   body's surface zone ([`surface_zone`]), and free-falls otherwise, its origin following
//!   the focus (a floating origin).
//! - **Range**: vessels farther than [`RANGE_OUT`] from the focus are packed, those within
//!   [`RANGE_IN`] are brought back.

pub mod bodies;
pub mod bubble;
pub mod pose;
pub mod predict;
mod step;
pub mod terrains;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_support;
pub mod vessels;

use crate::TerrainGround;
use bodies::{Bodies, Minor};
use bubble::Bubble;
use creeer_data::Data;
use creeer_math::glam::DVec3;
use creeer_orbit::generate::Generated;
use creeer_physics::world::Body;
use creeer_physics::world::ground::Ground;
use pose::Pose;
use std::sync::{Arc, Mutex, PoisonError};
use terrains::{BodyRef, Terrains};
use vessels::{Vessel, Where};

/// Fastest warp at which the bubble's physics still runs (physics steps per tick).
pub const MAX_PHYSICS_WARP: f64 = 4.0;
/// Fastest warp.
pub const MAX_WARP: f64 = 1e7;
/// Vessels nearer than this to the focus join the bubble (m).
pub const RANGE_IN: f64 = 2_500.0;
/// Vessels farther than this from the focus leave it (m).
pub const RANGE_OUT: f64 = 3_000.0;
/// In an orbit frame, the origin moves to the focus once it is this far from it (m) ...
pub const REBASE_DISTANCE: f64 = 1_000.0;
/// ... or moving this fast relative to it (m/s).
pub const REBASE_SPEED: f64 = 20.0;
/// The ephemeris is kept this far ahead of the present (s).
pub(crate) const LOOKAHEAD: f64 = 3_600.0;

/// Height above a body's radius up to which the bubble turns with it (m): 15 % of the
/// radius, at least 10 km (75 km on the 500 km home planet). Leaving takes 20 % more.
pub fn surface_zone(radius: f64) -> f64 {
    (0.15 * radius).max(10_000.0)
}

/// Something that happened during a tick.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A vessel on rails hit a body and now rests where it hit.
    VesselImpact {
        /// Vessel id.
        vessel: u64,
        /// The body.
        body: BodyRef,
    },
    /// A minor body hit a major one (it stops there).
    MinorImpact {
        /// Index of the minor.
        minor: usize,
        /// Index of the major.
        body: usize,
    },
    /// Two major bodies touch (not resolved yet: the simulation reports it and stops warp).
    MajorContact {
        /// Their indices.
        bodies: (usize, usize),
    },
    /// The bubble changed frame.
    FrameChanged,
}

/// What the bubble follows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Focus {
    /// A vessel in the bubble.
    Vessel(u64),
    /// A point in bubble coordinates (the free camera).
    Point(DVec3),
}

/// The universe.
#[derive(Debug)]
pub struct Universe {
    /// Name of the system.
    pub name: String,
    /// The celestial bodies.
    pub bodies: Bodies,
    /// For every major, the body it was generated around (map view hierarchy).
    pub parents: Vec<Option<usize>>,
    /// Index of the home planet.
    pub home: usize,
    /// The vessels.
    pub vessels: Vec<Vessel>,
    /// The physics bubble.
    pub bubble: Bubble,
    /// What the bubble follows.
    pub focus: Focus,
    /// Terrain of the bodies with a surface (shared with the renderer's streamer, so it sees
    /// the edits).
    pub terrains: Arc<Mutex<Terrains>>,
    /// Time warp factor.
    pub warp: f64,
    /// Events since they were last taken.
    pub events: Vec<Event>,
    time: f64,
    next_id: u64,
}

impl Universe {
    /// A universe from a generated system, with the bubble on the home planet's surface;
    /// `data` the packs its terrains come from, `dt` the physics step.
    pub fn new(system: Generated, data: Arc<Data>, dt: f64) -> Self {
        let home_file = system.majors[system.home]
            .body
            .surface
            .clone()
            .unwrap_or_default();
        let bodies = Bodies {
            ephemeris: system.ephemeris(),
            minors: system
                .minor_groups()
                .into_iter()
                .zip(&system.minors)
                .map(|(group, placed)| Minor {
                    body: placed.body.clone(),
                    group,
                })
                .collect(),
        };
        let terrains = Arc::new(Mutex::new(Terrains::new(data, home_file)));
        let home = BodyRef::Major(system.home);
        let ground = ground(&terrains, &bodies, home);
        let radius = bodies.body(home).radius;
        Self {
            name: system.name,
            parents: system.parents,
            home: system.home,
            bubble: Bubble::on_surface(home, dt, ground, 0.0),
            bodies,
            vessels: Vec::new(),
            // On the ground of home, at its north pole.
            focus: Focus::Point(DVec3::Y * radius),
            terrains,
            warp: 1.0,
            events: Vec::new(),
            time: 0.0,
            next_id: 1,
        }
    }

    /// Current time (s since the system's epoch).
    pub fn time(&self) -> f64 {
        self.time
    }

    /// The bubble's frame now.
    pub fn bubble_pose(&self) -> Pose {
        self.bubble.pose(&self.bodies, self.time)
    }

    /// Adds a body to the bubble (bubble coordinates); returns its vessel id.
    pub fn spawn(&mut self, name: &str, body: Body) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        // Its first acceleration comes from the field of this frame at this time.
        let field = self.bubble.field(&self.bodies, self.time);
        self.bubble.world.set_planet(Some(field));
        self.bubble.world.add(body);
        self.bubble.vessels.push(id);
        self.vessels.push(Vessel {
            id,
            name: name.to_owned(),
            place: Where::Bubble,
        });
        id
    }

    /// The vessel with id `id`.
    pub fn vessel(&self, id: u64) -> Option<&Vessel> {
        self.vessels.iter().find(|v| v.id == id)
    }

    /// Index of a vessel's body in the bubble's world.
    pub fn bubble_index(&self, id: u64) -> Option<usize> {
        self.bubble.vessels.iter().position(|&v| v == id)
    }

    /// Removes every vessel.
    pub fn clear_vessels(&mut self) {
        self.bubble.world.retain_bodies(|_, _| false);
        self.bubble.vessels.clear();
        self.vessels.clear();
    }

    /// Inertial position and velocity of the focus now.
    pub fn focus_state(&self) -> (DVec3, DVec3) {
        let (x, v) = self.focus_local();
        self.bubble_pose().to_inertial(x, v)
    }

    /// The focus in bubble coordinates (position, velocity).
    pub fn focus_local(&self) -> (DVec3, DVec3) {
        match self.focus {
            Focus::Vessel(id) => self.bubble_index(id).map_or_else(
                || (DVec3::ZERO, DVec3::ZERO),
                |i| {
                    let (x, v, _) = vessels::centre_of_mass(&self.bubble.world.bodies[i]);
                    (x, v)
                },
            ),
            Focus::Point(p) => (p, DVec3::ZERO),
        }
    }

    /// Pushes a body with an inertial force from now on for `duration` seconds.
    pub fn push_body(&mut self, body: BodyRef, force: DVec3, duration: f64) {
        let t = self.time;
        self.bodies.push(body, t, t + duration, force, LOOKAHEAD);
    }

    /// Takes the events since the last call.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}

/// Collision ground for a surface frame on `body`.
fn ground(terrains: &Mutex<Terrains>, bodies: &Bodies, body: BodyRef) -> Option<Arc<dyn Ground>> {
    terrains
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(body, bodies.body(body))
        .map(|t| Arc::new(TerrainGround(t)) as Arc<dyn Ground>)
}
