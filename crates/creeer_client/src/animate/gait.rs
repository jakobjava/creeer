//! A person's gait at a speed, from measurements (G1.3g, ADR-0026 decision 9): how often they
//! step, how long each foot stays on the ground, and when they run.
//!
//! - **Walking:** step length over cadence, the walk ratio, holds at about 0.0065 m per
//!   step/min across free walking from 0.9 to 2.3 m/s (Sekiya, Nagasaki, Ito and Furuna 1996;
//!   0.006–0.007 in later work). So speed v = ratio · cadence² / 60, and the cadence is
//!   √(60·v / ratio): 114 steps/min at 1.4 m/s, 0.74 m a step.
//! - **Running:** strides a minute 75.01 + 3.006·v (m/s), the group fit of 1.64–4.68 m/s from
//!   wearable data ("Inter-individual differences in stride frequencies during running obtained
//!   from wearable data", J. Sports Sci. 2019, PubMed 31079578).
//! - **From one to the other** at a Froude number v²/(g·L) of 0.5, as people of every leg
//!   length switch (Kram, Domingo and Ferris 1997): 2.2 m/s for the engineer's 0.96 m legs.
//! - **On the ground:** a foot stands 60 % of a walking stride (Perry's gait cycle) and about
//!   35 % of a running one, which has its flight phase.

use creeer_sim::character::bones;

/// Step length over cadence (m per step/min) while walking.
pub const WALK_RATIO: f64 = 0.0065;
/// Running's strides a minute: a + b·v.
pub const RUN_STRIDES: (f64, f64) = (75.01, 3.006);
/// The Froude number people change gait at.
pub const TRANSITION_FROUDE: f64 = 0.5;
/// The share of a stride a foot is on the ground, walking and running.
pub const WALK_DUTY: f64 = 0.6;
pub const RUN_DUTY: f64 = 0.35;
/// Standard gravity (m/s²).
const G: f64 = 9.806_65;
/// Below this speed the walk ratio no longer holds (it grows, and varies): steps are taken
/// at the cadence of this speed, shorter.
const SLOWEST: f64 = 0.6;
/// The width of speeds the gaits blend over, either side of the transition (m/s).
const BLEND: f64 = 0.15;

/// The legs' length: the hip joints' height standing (m).
pub fn leg_length() -> f64 {
    bones::joints()[5].2.y
}

/// The speed people change from walking to running (m/s).
pub fn transition_speed() -> f64 {
    (TRANSITION_FROUDE * G * leg_length()).sqrt()
}

/// A gait at a speed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gait {
    /// Steps a second (two to a stride).
    pub cadence: f64,
    /// The share of a stride each foot is on the ground.
    pub duty: f64,
    /// How much it runs: 0 walking, 1 running.
    pub running: f64,
}

impl Gait {
    /// A step's length at `speed` (m).
    pub fn step_length(&self, speed: f64) -> f64 {
        speed / self.cadence
    }
}

/// Steps a minute walking at `speed` (m/s).
pub fn walking_cadence(speed: f64) -> f64 {
    (60.0 * speed.max(SLOWEST) / WALK_RATIO).sqrt()
}

/// Steps a minute running at `speed` (m/s).
pub fn running_cadence(speed: f64) -> f64 {
    2.0 * (RUN_STRIDES.0 + RUN_STRIDES.1 * speed)
}

/// The gait at `speed` (m/s).
pub fn at(speed: f64) -> Gait {
    let t = transition_speed();
    let x = ((speed - (t - BLEND)) / (2.0 * BLEND)).clamp(0.0, 1.0);
    let running = x * x * (3.0 - 2.0 * x);
    let per_minute = walking_cadence(speed) * (1.0 - running) + running_cadence(speed) * running;
    Gait {
        cadence: per_minute / 60.0,
        duty: WALK_DUTY * (1.0 - running) + RUN_DUTY * running,
        running,
    }
}
