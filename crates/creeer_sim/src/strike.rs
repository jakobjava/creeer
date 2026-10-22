//! Strikes (G1.4e, ADR-0027 decision 5): a tool swung at full effort, how fast its head goes and
//! what its blow does to what it hits.
//!
//! How fast a person swings an implement depends on its moment of inertia about the handle's
//! end, not its mass: speed falls as that inertia to the power 0.27 (Cross and Bower 2006, rods
//! swung standing; softball bats 0.25, golf clubs 0.26). The constant is a person's strength:
//! here a man's of the forensic volunteers who struck down at a target as hard as they could with
//! a 1 kg steel rod 65 cm long, 23.9 m/s on average (Muggenthaler et al. 2018; men 14.0–35.5 m/s
//! over rods of 0.5–1.5 kg and 40–90 cm, Trinh et al. 2018), taken at its end. The blow is a
//! collision of the tool's effective mass at its striking point (Cross: 1/Mₑ = 1/M + b²/I_cm)
//! with the struck body's at the point struck. A swing takes the 2 s of the most efficient rhythm
//! measured for an axe or a hammer, 25–35 strokes a minute (Hannover-Münden's forestry
//! physiology, FAO); its downswing as long as the spin takes to build through 120° evenly.

use crate::tools::Tool;
use creeer_math::det;

/// The volunteers' rod (1 kg, 0.65 m): its moment of inertia about its end (kg·m², a uniform
/// rod's mL²/3) and its spin at impact (rad/s: 23.9 m/s at its end).
const ROD_INERTIA: f64 = 1.0 * 0.65 * 0.65 / 3.0;
const ROD_SPIN: f64 = 23.9 / 0.65;
/// How the spin falls with the moment of inertia about the handle's end.
const LAW: f64 = 0.27;
/// Through how much the tool turns striking (rad): from overhead to before the body.
const DOWNSWING: f64 = 120.0 * std::f64::consts::PI / 180.0;
/// One swing, raised, struck and the tool freed and set again (s): 30 strokes a minute.
pub const CYCLE: f64 = 2.0;
/// From the swing's start to its blow (s): half the cycle (not measured: the rest frees the
/// tool and sets it again).
pub const TO_BLOW: f64 = 0.5 * CYCLE;

/// A tool's swing at full effort.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swing {
    /// Its spin at impact (rad/s), and its striking point's speed (m/s).
    pub spin: f64,
    pub speed: f64,
    /// Its kinetic energy then (J).
    pub energy: f64,
    /// Its effective mass at the striking point (kg).
    pub mass: f64,
    /// How long it is raised for, and then struck (s).
    pub raise: f64,
    pub strike: f64,
}

/// `tool` swung at full effort.
pub fn swing(tool: &Tool) -> Swing {
    let inertia = tool.mass * tool.gyration * tool.gyration;
    let spin = ROD_SPIN * det::pow(ROD_INERTIA / inertia, LAW);
    // About its centre of mass, and the striking point's lever there.
    let centre = inertia - tool.mass * tool.carry * tool.carry;
    let lever = tool.strike - tool.carry;
    let strike = 2.0 * DOWNSWING / spin;
    Swing {
        spin,
        speed: spin * tool.strike,
        energy: 0.5 * inertia * spin * spin,
        mass: 1.0 / (1.0 / tool.mass + lever * lever / centre),
        raise: (TO_BLOW - strike).max(0.0),
        strike,
    }
}

/// The impulse a blow at `speed` (m/s) of a tool of effective mass `tool` gives a body of
/// effective mass `body` there (N·s), bouncing back by `restitution`.
pub fn impulse(speed: f64, tool: f64, body: f64, restitution: f64) -> f64 {
    (1.0 + restitution) * speed / (1.0 / tool + 1.0 / body)
}

#[cfg(test)]
#[path = "strike_tests.rs"]
mod tests;
