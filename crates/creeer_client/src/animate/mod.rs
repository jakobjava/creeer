//! Procedural animation on the client (G1.3g, ADR-0026 decision 9; G1.4a, ADR-0027): each
//! character's bones from its simulated body, its motion and the ground. The trunk is the
//! simulated body (its up, where it is), turned to the way it moves (the body itself, a box,
//! never turns) and leaning into its speed and acceleration; the legs walk or run at the
//! cadence measured people use at its speed (`gait.rs`), their feet planted where they land and
//! placed by inverse kinematics on the ground found under them, the hips lowered where a foot
//! cannot reach (Johansen 2009); the arms swing against the legs as measured (`upper.rs`). A
//! change between standing, stepping, the air and the water is smoothed by inertialization
//! (`pose.rs`). Fallen, the bones are the ragdoll's segments exactly; standing up blends from
//! there. Nothing is sent: it is drawn from what the client already has.

mod arms;
pub mod feet;
pub mod gait;
pub mod grasp;
pub mod hands;
mod hips;
mod holding;
mod inertial;
mod pose;
pub mod skeleton;
mod stand;
mod stop;
mod upper;

use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3};
use feet::{Foot, Ground};
use hands::{HandShape, Hold};
use pose::{Mode, Smoother};
use std::collections::HashMap;

/// Standing up from the ragdoll takes this long (s).
const RISE: f64 = 0.8;

/// A character to animate: where its body is and whether it has fallen.
#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// Its body's centre and orientation.
    Standing(DVec3, DQuat),
    /// Its ragdoll's bones.
    Fallen(Vec<Frame64>),
}

#[derive(Clone, Debug)]
struct State {
    centre: DVec3,
    velocity: DVec3,
    /// How fast it rises (m/s, as last seen) and how fast it gathers speed downward (m/s²,
    /// followed): in the air, as fast as it falls.
    climb: f64,
    sinking: f64,
    /// The time since the body was last seen to move (s).
    unseen: f64,
    /// It jumped as last seen.
    leapt: bool,
    /// How fast it rises in flight (m/s), as a body falling freely from its leap.
    flight: f64,
    heading: DVec3,
    /// Its acceleration along the way it faces (m/s²), as the trunk leans into it.
    ahead: f64,
    /// Through the stride: 0 the right foot lands, 0.5 the left.
    phase: f64,
    feet: Option<[Foot; 2]>,
    time: f64,
    /// The last bones drawn, and standing up: the ragdoll's bones it rises from, and how long.
    last: Vec<Frame64>,
    rising: Option<(Vec<Frame64>, f64)>,
    fallen: bool,
    /// Stepping (not standing still), and stopping (the last step on, at `pace`, m/s).
    stepping: bool,
    stopping: bool,
    /// Stopping: each foot has been in the air since the stop began (one landing ends it).
    stop_up: [bool; 2],
    pace: f64,
    /// How the arms carry themselves: 0 walking, 1 running.
    carriage: f64,
    /// How far the hips were lowered last (m), and how fast they are going down (m/s).
    drop: f64,
    lowering: f64,
    /// A tool's swing, through it (`Animator::swing`).
    swing: Option<f64>,
    /// What the hands hold (G2.10, `holding.rs`), and whether they still do (let go, the arms
    /// come back from it); how far the arms reach for it (0 to 1); each hand's hold on it, in
    /// its frame; the fingers' holds drawn (right, left); the stoop's lean (rad) and the hips'
    /// drop (m), each with its rate; the hands' shapes at rest.
    held: Option<grasp::Load>,
    holding: bool,
    reaching: f64,
    grasps: [Option<grasp::Reach>; 2],
    holds: [Hold; 2],
    stoop: [f64; 4],
    hands: [HandShape; 2],
    /// What it did last frame, and its poses smoothed across changes.
    mode: Mode,
    smoother: Smoother,
}

/// Every character's animation, by a key of the caller's (an entity).
#[derive(Debug, Default)]
pub struct Animator {
    states: HashMap<u32, State>,
    /// The hands' shapes at rest, the model's (`Hands::shapes`); none: the ragdoll's.
    hands: Option<[HandShape; 2]>,
}

impl Animator {
    /// The model's hands from now on (their arms are placed by where they touch what they hold).
    pub fn set_hands(&mut self, hands: [HandShape; 2]) {
        self.hands = Some(hands);
    }

    /// Character `key`'s bones this frame, `dt` seconds after the last, its `body` as given.
    pub fn bones(&mut self, key: u32, body: &Body, dt: f64, ground: &dyn Ground) -> Vec<Frame64> {
        let state = self.states.entry(key).or_insert_with(|| State::new(body));
        if let Some(hands) = self.hands {
            state.hands = hands;
        }
        state.time += dt;
        let bones = match body {
            Body::Fallen(bones) => {
                state.fallen = true;
                state.feet = None;
                bones.clone()
            }
            Body::Standing(centre, orientation) => {
                if state.fallen {
                    state.fallen = false;
                    state.rising = Some((state.last.clone(), 0.0));
                    state.centre = *centre;
                    state.velocity = DVec3::ZERO;
                }
                let standing = state.stand(*centre, *orientation, dt, ground);
                match &mut state.rising {
                    Some((from, t)) => {
                        *t += dt;
                        let x = (*t / RISE).min(1.0);
                        let blended = blend(from, &standing, x * x * (3.0 - 2.0 * x));
                        if x >= 1.0 {
                            state.rising = None;
                        }
                        blended
                    }
                    None => standing,
                }
            }
        };
        state.last.clone_from(&bones);
        bones
    }

    /// Character `key`'s tool swing, `progress` through it (−1 its pose, 0 raised, 1 struck, 2
    /// its pose again; none: no swing; `upper::swing`), timed to the server's strikes (G1.4e).
    /// The right arm follows it.
    pub fn swing(&mut self, key: u32, progress: Option<f64>) {
        if let Some(state) = self.states.get_mut(&key) {
            state.swing = progress;
        }
    }

    /// What character `key`'s hands hold this frame (G2.10), if anything: its arms reach where
    /// they hold it, and it stoops to it held low. A load not the one held before is taken anew
    /// (the hands placed on it at once, the arms reaching out to it).
    pub fn holding(&mut self, key: u32, load: Option<grasp::Load>) {
        if let Some(state) = self.states.get_mut(&key) {
            state.holding = load.is_some();
            if let Some(load) = load {
                if state.held.is_some_and(|h| h.id != load.id) {
                    state.grasps = [None; 2];
                }
                state.held = Some(load);
            }
        }
    }

    /// How character `key`'s fingers hold this frame (right, left).
    pub fn holds(&self, key: u32) -> [Hold; 2] {
        self.states
            .get(&key)
            .map_or([Hold::Relaxed; 2], |s| s.holds)
    }

    /// Forgets the characters not in `keep`.
    pub fn retain(&mut self, keep: &[u32]) {
        self.states.retain(|k, _| keep.contains(k));
    }
}

/// Each bone from `a` towards `b` by `x`.
fn blend(a: &[Frame64], b: &[Frame64], x: f64) -> Vec<Frame64> {
    a.iter()
        .zip(b)
        .map(|(a, b)| {
            Frame64::new(
                a.position.lerp(b.position, x),
                a.rotation.slerp(b.rotation, x),
            )
        })
        .collect()
}

impl State {
    fn new(body: &Body) -> Self {
        let (centre, orientation) = match body {
            Body::Standing(c, q) => (*c, *q),
            Body::Fallen(b) => (b[0].position, b[0].rotation),
        };
        Self {
            centre,
            velocity: DVec3::ZERO,
            climb: 0.0,
            sinking: 0.0,
            unseen: 0.0,
            leapt: false,
            flight: 0.0,
            heading: orientation * DVec3::Z,
            ahead: 0.0,
            phase: 0.0,
            feet: None,
            time: 0.0,
            last: Vec::new(),
            rising: None,
            fallen: matches!(body, Body::Fallen(_)),
            stepping: false,
            stopping: false,
            pace: 0.0,
            carriage: 0.0,
            drop: 0.0,
            lowering: 0.0,
            swing: None,
            held: None,
            holding: false,
            reaching: 0.0,
            grasps: [None; 2],
            holds: [Hold::Relaxed; 2],
            stoop: [0.0; 4],
            hands: [0, 1].map(HandShape::approximate),
            stop_up: [false; 2],
            mode: Mode::Standing,
            smoother: Smoother::default(),
        }
    }
}

#[cfg(test)]
#[path = "animate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "probe_tests.rs"]
mod probe_tests;
