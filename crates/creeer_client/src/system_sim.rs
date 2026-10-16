//! F4 solar-system sandbox simulation: the universe on its own thread (server-authoritative,
//! plan.md §1), steered by commands from the UI and published as snapshots.

use crate::physics_sandbox::PhysicsSnapshot;
use crate::system_cmd::{self, Command, Engine};
use creeer_core::Simulation;
use creeer_math::glam::{DQuat, DVec3};
use creeer_orbit::Body;
use creeer_orbit::kepler::Elements;
use creeer_physics::world::frame::FrameChange;
use creeer_physics::world::shape::Shape;
use creeer_sim::universe::pose::Pose;
use creeer_sim::universe::predict::{PlotFrame, Prediction};
use creeer_sim::universe::terrains::{BodyRef, Terrains};
use creeer_sim::universe::vessels::{Where, centre_of_mass};
use creeer_sim::universe::{Event, Focus, Universe};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

/// The UI's handle for sending commands.
pub type Commands = Arc<Mutex<Vec<Command>>>;

/// A celestial body as the views need it.
#[derive(Clone, Debug)]
pub struct BodyView {
    /// Which body.
    pub which: BodyRef,
    /// Its properties.
    pub body: Body,
    /// Inertial position (m) and velocity (m/s).
    pub state: (DVec3, DVec3),
    /// Body-fixed → inertial rotation.
    pub rotation: DQuat,
    /// The major it was generated around.
    pub parent: Option<usize>,
}

/// The focused vessel.
#[derive(Clone, Debug)]
pub struct FocusView {
    /// Vessel id.
    pub id: u64,
    /// Name.
    pub name: String,
    /// Inertial position and velocity.
    pub state: (DVec3, DVec3),
    /// The major it orbits.
    pub primary: usize,
    /// Osculating elements about the primary.
    pub elements: Elements,
    /// Height above the primary's mean radius (m).
    pub altitude: f64,
    /// Speed relative to the primary (m/s).
    pub speed: f64,
    /// Mass (kg).
    pub mass: f64,
}

/// One tick of the universe for the views.
#[derive(Clone, Debug)]
pub struct SystemSnapshot {
    /// Game time (s).
    pub time: f64,
    /// Time warp achieved.
    pub warp: f64,
    /// Time warp asked for (the governor lowers it while ticks would take too long).
    pub requested_warp: f64,
    /// Duration of the last tick (ms).
    pub step_ms: f64,
    /// The bubble's frame now.
    pub pose: Pose,
    /// Counts frame changes; `change` is the last one.
    pub generation: u64,
    /// The last frame change.
    pub change: Option<FrameChange>,
    /// The body a surface frame turns with.
    pub anchor: Option<BodyRef>,
    /// Every body.
    pub bodies: Arc<Vec<BodyView>>,
    /// Bodies in the bubble (bubble coordinates).
    pub physics: PhysicsSnapshot,
    /// Packed vessels: id, name, inertial position, landed.
    pub stored: Vec<(u64, String, DVec3, bool)>,
    /// The focused vessel.
    pub focus: Option<FocusView>,
    /// Its predicted path and the frame it is drawn in.
    pub prediction: Arc<(PlotFrame, Prediction)>,
    /// Lagrange points of the focus's primary with its own primary (inertial), if any.
    pub lagrange: Option<(usize, usize, [DVec3; 5])>,
    /// Recent events and messages.
    pub log: Arc<Vec<String>>,
    /// Terrain shared with the renderer.
    pub terrains: Arc<Mutex<Terrains>>,
}

/// The universe and what the UI asked for.
#[derive(Debug)]
pub struct SystemSim {
    /// The universe.
    pub universe: Universe,
    commands: Commands,
    engine: Engine,
    plot: Option<PlotFrame>,
    prediction: Arc<(PlotFrame, Prediction)>,
    log: Vec<String>,
    step_ms: f64,
    ticks: u64,
    /// Warp asked for.
    requested_warp: f64,
}

impl SystemSim {
    /// The sandbox on `universe`; returns it with its command inbox.
    pub fn new(universe: Universe) -> (Self, Commands) {
        let commands: Commands = Arc::default();
        let plot = PlotFrame::Body(BodyRef::Major(universe.home));
        let sim = Self {
            universe,
            commands: Arc::clone(&commands),
            engine: Engine::default(),
            plot: None,
            prediction: Arc::new((plot, Prediction::default())),
            log: vec!["welcome: spawn a ship in orbit to fly".into()],
            step_ms: 0.0,
            ticks: 0,
            requested_warp: 1.0,
        };
        (sim, commands)
    }

    fn say(&mut self, text: String) {
        self.log.push(text);
        if self.log.len() > 8 {
            self.log.remove(0);
        }
    }

    /// The focused vessel's state, primary and mass, if it is in the bubble.
    fn focus_state(&self) -> Option<(u64, (DVec3, DVec3), usize, f64)> {
        let u = &self.universe;
        let Focus::Vessel(id) = u.focus else {
            return None;
        };
        let i = u.bubble_index(id)?;
        let (x, v, mass) = centre_of_mass(&u.bubble.world.bodies[i]);
        let state = u.bubble_pose().to_inertial(x, v);
        Some((id, state, u.primary_of(state.0), mass))
    }

    /// Applies the engine of the focused vessel for the coming tick.
    fn apply_engine(&mut self) {
        let Some((id, (x, v), primary, _)) = self.focus_state() else {
            return;
        };
        let u = &mut self.universe;
        let Some(i) = u.bubble_index(id) else { return };
        let (px, pv) = u.bodies.ephemeris.state(primary, u.time());
        let force = if self.engine.on {
            system_cmd::direction(self.engine.direction, x - px, v - pv) * self.engine.thrust
        } else {
            DVec3::ZERO
        };
        let local = u.bubble_pose().rotation.conjugate() * force;
        let body = &mut u.bubble.world.bodies[i];
        if body.structure.load[0] != local {
            body.structure.load[0] = local;
            body.wake();
        }
    }

    /// Recomputes the focus's predicted path (every half second of real time).
    fn update_prediction(&mut self) {
        let Some((_, state, primary, _)) = self.focus_state() else {
            return;
        };
        let u = &self.universe;
        let mu = u.bodies.ephemeris.bodies()[primary].mu;
        let (px, pv) = u.bodies.ephemeris.state(primary, u.time());
        let el = Elements::from_state(mu, state.0 - px, state.1 - pv);
        // One and a half orbits, at most 60 days; open orbits: 10 days.
        let duration = if el.e < 1.0 {
            (1.5 * el.period()).min(60.0 * 86_400.0)
        } else {
            10.0 * 86_400.0
        };
        let frame = self
            .plot
            .unwrap_or(PlotFrame::Body(BodyRef::Major(primary)));
        let prediction = self.universe.predict(state, duration, 600, frame);
        self.prediction = Arc::new((frame, prediction));
    }
}

impl Simulation for SystemSim {
    type Snapshot = SystemSnapshot;

    fn step(&mut self, _dt: f64) {
        let commands =
            std::mem::take(&mut *self.commands.lock().unwrap_or_else(PoisonError::into_inner));
        for command in commands {
            let message = system_cmd::apply(self, command);
            if let Some(m) = message {
                self.say(m);
            }
        }
        self.apply_engine();
        let start = Instant::now();
        self.universe.step();
        self.step_ms = start.elapsed().as_secs_f64() * 1e3;
        self.govern_warp();
        for event in self.universe.take_events() {
            let text = match event {
                Event::VesselImpact { vessel, .. } => format!("vessel {vessel} hit the ground"),
                Event::MinorImpact { minor, body } => format!("asteroid {minor} hit body {body}"),
                Event::MajorContact { bodies } => {
                    format!("bodies {} and {} collide!", bodies.0, bodies.1)
                }
                Event::FrameChanged => continue,
            };
            self.say(text);
        }
        self.ticks += 1;
        if self.ticks % 30 == 1 {
            self.update_prediction();
        }
    }

    fn snapshot(&self) -> SystemSnapshot {
        let u = &self.universe;
        let t = u.time();
        let bodies = u
            .bodies
            .all()
            .map(|(which, b)| BodyView {
                which,
                body: b.clone(),
                state: u.bodies.state(which, t),
                rotation: b.spin.orientation_at(t),
                parent: match which {
                    BodyRef::Major(i) => u.parents[i],
                    BodyRef::Minor(_) => Some(0),
                },
            })
            .collect();
        let physics = system_cmd::physics_snapshot(u, self.step_ms);
        let stored = u
            .vessels
            .iter()
            .filter_map(|v| {
                let landed = matches!(v.place, Where::Landed { .. });
                creeer_sim::universe::vessels::stored_position(&v.place, &u.bodies, t)
                    .map(|p| (v.id, v.name.clone(), p, landed))
            })
            .collect();
        let focus = self.focus_state().map(|(id, state, primary, mass)| {
            let p = &u.bodies.ephemeris.bodies()[primary];
            let (px, pv) = u.bodies.ephemeris.state(primary, t);
            let rel = (state.0 - px, state.1 - pv);
            FocusView {
                id,
                name: u.vessel(id).map_or_else(String::new, |v| v.name.clone()),
                state,
                primary,
                elements: Elements::from_state(p.mu, rel.0, rel.1),
                altitude: rel.0.length() - p.radius,
                speed: rel.1.length(),
                mass,
            }
        });
        // The pair the map turns with, or else the focus's primary and its own primary.
        let pair = match self.plot {
            Some(PlotFrame::Corotating(p, q)) => Some((p, q)),
            _ => focus
                .as_ref()
                .and_then(|f| u.parents[f.primary].map(|p| (p, f.primary))),
        };
        let lagrange = pair.map(|(p, q)| (p, q, u.lagrange_points(p, q)));
        SystemSnapshot {
            time: t,
            warp: u.warp,
            requested_warp: self.requested_warp,
            step_ms: self.step_ms,
            pose: u.bubble_pose(),
            generation: u.bubble.generation,
            change: u.bubble.last_change,
            anchor: u.bubble.anchor(),
            bodies: Arc::new(bodies),
            physics,
            stored,
            focus,
            prediction: Arc::clone(&self.prediction),
            lagrange,
            log: Arc::new(self.log.clone()),
            terrains: Arc::clone(&u.terrains),
        }
    }
}

impl SystemSim {
    /// Engine settings (for the command module).
    pub(crate) fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// Plotting frame of predictions (`None`: around the focus's primary).
    pub(crate) fn set_plot(&mut self, plot: Option<PlotFrame>) {
        self.plot = plot;
        self.ticks = 0; // recompute at once
    }
}

/// A rigid box ship of `mass` with half-extents `half`.
pub fn ship_body(position: DVec3, mass: f64, dt: f64) -> creeer_physics::world::Body {
    let half = DVec3::new(1.5, 1.5, 3.0);
    let mut body = creeer_physics::world::Body::rigid(Shape::Box { half }, mass, position, dt);
    body.friction = 0.6;
    body
}

/// Ticks above this cost (ms) lower the warp; below the lower one it rises again.
const WARP_SLOW_MS: f64 = 12.0;
const WARP_FAST_MS: f64 = 6.0;

impl SystemSim {
    /// Asks for a time warp (the governor may grant less).
    pub(crate) fn request_warp(&mut self, warp: f64) -> Result<(), String> {
        self.universe.set_warp(warp)?;
        self.requested_warp = self.universe.warp;
        Ok(())
    }

    /// The warp governor: at high warp every ship on rails is integrated through every orbit
    /// it flies, which nothing may skip; when that would take longer than a tick allows, the
    /// warp is lowered instead (and raised again when there is room), so the frame rate stays
    /// smooth and the physics exact.
    fn govern_warp(&mut self) {
        let u = &mut self.universe;
        if u.warp <= creeer_sim::universe::MAX_PHYSICS_WARP {
            if self.requested_warp > creeer_sim::universe::MAX_PHYSICS_WARP {
                // An event (an impact, a collision) stopped the warp.
                self.requested_warp = u.warp;
            }
            return;
        }
        let floor = creeer_sim::universe::MAX_PHYSICS_WARP * 2.0;
        if self.step_ms > WARP_SLOW_MS {
            u.warp = (u.warp * (WARP_SLOW_MS / self.step_ms).max(0.5)).max(floor);
        } else if self.step_ms < WARP_FAST_MS && u.warp < self.requested_warp {
            u.warp = (u.warp * 1.25).min(self.requested_warp);
        }
    }
}
