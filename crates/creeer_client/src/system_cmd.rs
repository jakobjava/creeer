//! What the UI can ask of the solar-system simulation, and how it is carried out (in the
//! simulation tick, server-authoritative).

use crate::physics_sandbox::PhysicsSnapshot;
use crate::system_sim::{SystemSim, ship_body};
use creeer_math::glam::{DQuat, DVec3};
use creeer_physics::world::Body;
use creeer_physics::world::shape::Shape;
use creeer_sim::universe::predict::PlotFrame;
use creeer_sim::universe::terrains::BodyRef;
use creeer_sim::universe::{Focus, Universe};
use creeer_terrain::edits::Edit;

/// A direction relative to the orbit about a primary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    /// Along the velocity.
    #[default]
    Prograde,
    /// Against it.
    Retrograde,
    /// Along the orbit's angular momentum.
    Normal,
    /// Against it.
    Antinormal,
    /// Away from the primary.
    RadialOut,
    /// Towards it.
    RadialIn,
}

impl Direction {
    /// All directions with their names.
    pub const ALL: [(Self, &'static str); 6] = [
        (Self::Prograde, "prograde"),
        (Self::Retrograde, "retrograde"),
        (Self::Normal, "normal"),
        (Self::Antinormal, "anti-normal"),
        (Self::RadialOut, "radial out"),
        (Self::RadialIn, "radial in"),
    ];
}

/// Unit vector of `direction` for relative position `r` and velocity `v`.
pub fn direction(direction: Direction, r: DVec3, v: DVec3) -> DVec3 {
    let normal = r.cross(v).normalize_or_zero();
    match direction {
        Direction::Prograde => v.normalize_or_zero(),
        Direction::Retrograde => -v.normalize_or_zero(),
        Direction::Normal => normal,
        Direction::Antinormal => -normal,
        Direction::RadialOut => r.normalize_or_zero(),
        Direction::RadialIn => -r.normalize_or_zero(),
    }
}

/// The focused ship's engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Engine {
    /// Thrust (N).
    pub thrust: f64,
    /// Direction relative to the orbit.
    pub direction: Direction,
    /// Firing.
    pub on: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            thrust: 2e5,
            direction: Direction::Prograde,
            on: false,
        }
    }
}

/// A request from the UI.
#[derive(Clone, Debug)]
pub enum Command {
    /// Change the time warp.
    Warp(f64),
    /// A 20 t ship in a circular orbit `altitude` above the body below the focus.
    ShipInOrbit(f64),
    /// A crate at a bubble position.
    Crate(DVec3),
    /// A ball at a bubble position.
    Ball(DVec3),
    /// Follow the free camera at this bubble position.
    FocusPoint(DVec3),
    /// Engine settings of the focused ship.
    Engine(Engine),
    /// Push a body: force (N) in a direction relative to its own orbit, for a duration (s).
    Push(BodyRef, f64, Direction, f64),
    /// Plotting frame of predictions (`None`: the focus's primary).
    Plot(Option<PlotFrame>),
    /// Change the terrain of the body the bubble stands on.
    Edit(Edit),
    /// Remove every vessel.
    Clear,
}

/// Carries out a command; returns a message for the log.
pub fn apply(sim: &mut SystemSim, command: Command) -> Option<String> {
    match command {
        Command::Warp(w) => sim
            .request_warp(w)
            .err()
            .map(|e| format!("cannot warp: {e}")),
        Command::ShipInOrbit(altitude) => Some(spawn_ship(&mut sim.universe, altitude)),
        Command::Crate(at) => {
            let dt = sim.universe.bubble.world.dt();
            let mut body = Body::rigid(
                Shape::Box {
                    half: DVec3::splat(0.5),
                },
                200.0,
                at,
                dt,
            );
            body.structure.orientation[0] =
                DQuat::from_rotation_arc(DVec3::Y, up(&sim.universe, at));
            body.friction = 0.7;
            sim.universe.spawn("crate", body);
            None
        }
        Command::Ball(at) => {
            let dt = sim.universe.bubble.world.dt();
            let mut body = Body::rigid(Shape::Sphere { radius: 0.4 }, 300.0, at, dt);
            body.friction = 0.7;
            body.restitution = 0.3;
            sim.universe.spawn("ball", body);
            None
        }
        Command::FocusPoint(p) => {
            if !matches!(sim.universe.focus, Focus::Vessel(_)) {
                sim.universe.focus = Focus::Point(p);
            }
            None
        }
        Command::Engine(engine) => {
            *sim.engine_mut() = engine;
            None
        }
        Command::Push(body, force, dir, duration) => {
            Some(push(&mut sim.universe, body, force, dir, duration))
        }
        Command::Plot(plot) => {
            sim.set_plot(plot);
            None
        }
        Command::Edit(edit) => {
            let u = &mut sim.universe;
            let anchor = u.bubble.anchor()?;
            let terrain = u.terrains.lock().ok()?.loaded(anchor).cloned()?;
            let (lo, hi) = edit.brush().bounds();
            terrain.edit(edit);
            u.bubble
                .world
                .wake_near((lo + hi) * 0.5, (hi - lo).length() * 0.5 + 1.0);
            None
        }
        Command::Clear => {
            sim.universe.clear_vessels();
            sim.universe.focus = Focus::Point(DVec3::ZERO);
            None
        }
    }
}

/// Local up at a bubble position: away from the anchor's centre, or +Y in space.
fn up(u: &Universe, at: DVec3) -> DVec3 {
    if u.bubble.anchor().is_some() {
        at.normalize()
    } else {
        DVec3::Y
    }
}

/// Spawns a ship in a circular orbit over the body the focus is near, and follows it.
fn spawn_ship(u: &mut Universe, altitude: f64) -> String {
    let (fx, _) = u.focus_state();
    let primary = u.primary_of(fx);
    let t = u.time();
    let (px, pv) = u.bodies.ephemeris.state(primary, t);
    let body = &u.bodies.ephemeris.bodies()[primary];
    let r = body.radius + altitude;
    let out = (fx - px).normalize_or(DVec3::X);
    // Prograde about the body's own axis (its equator turns that way), or any other axis if
    // the focus is over a pole.
    let axis = body.spin.axis();
    let along = axis
        .cross(out)
        .normalize_or(DVec3::Y.cross(out).normalize_or(DVec3::Z));
    let speed = (body.mu / r).sqrt();
    let (x, v) = (px + out * r, pv + along * speed);
    let (local, local_v) = u.bubble_pose().from_inertial(x, v);
    let mut ship = ship_body(local, 2e4, u.bubble.world.dt());
    ship.structure.velocity[0] = local_v;
    let id = u.spawn("ship", ship);
    u.focus = Focus::Vessel(id);
    format!(
        "ship {id} in orbit {:.0} km above {}",
        altitude / 1e3,
        u.bodies.ephemeris.bodies()[primary].name
    )
}

/// Pushes `body` relative to its orbit about its primary.
fn push(u: &mut Universe, body: BodyRef, force: f64, dir: Direction, duration: f64) -> String {
    let t = u.time();
    let (x, v) = u.bodies.state(body, t);
    let primary = match body {
        BodyRef::Major(i) => u.parents[i].unwrap_or(0),
        BodyRef::Minor(_) => 0,
    };
    let (px, pv) = u.bodies.ephemeris.state(primary, t);
    let f = direction(dir, x - px, v - pv) * force;
    u.push_body(body, f, duration);
    let b = u.bodies.body(body);
    let dv = force * duration / b.mass();
    format!(
        "pushed {} with {force:.2e} N for {duration:.0} s: Δv {dv:.3e} m/s",
        b.name
    )
}

/// The bubble's bodies for drawing.
pub fn physics_snapshot(u: &Universe, step_ms: f64) -> PhysicsSnapshot {
    let mut snapshot = PhysicsSnapshot {
        time: u.time(),
        step_ms,
        ..PhysicsSnapshot::default()
    };
    let mut asleep = 0;
    for body in &u.bubble.world.bodies {
        asleep += usize::from(body.sleep.asleep);
        let s = &body.structure;
        for (node, shape) in &body.shapes {
            let node = *node;
            match *shape {
                Shape::Box { half } => {
                    snapshot
                        .boxes
                        .push((s.position[node], s.orientation[node], half));
                }
                Shape::Sphere { radius } => snapshot.spheres.push((s.position[node], radius)),
                // No hulls are made in the solar-system flight.
                Shape::Hull(_) => {}
            }
        }
    }
    snapshot.counts = (
        u.bubble.world.bodies.len(),
        asleep,
        u.bubble.world.contacts.len(),
    );
    snapshot
}
