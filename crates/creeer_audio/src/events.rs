//! What happened, as a sound to play (game thread; ADR-0037 decision 5): bodies meeting, a tool's
//! blow, a fastener driven home. Each works the sound out from the bodies' materials, shapes and
//! masses and the impulse between them (`modes.rs`, `acoustic.rs`); `happenings.rs` has the rest.

use crate::acoustic::{click, contact_modulus, contact_time};
use crate::library::{Driver, Ground, Library, Matter};
use crate::modes::{Blow, Modes, Shape, bar, cantilever};
use crate::voice::{Bus, Click, Excite, VoiceSpec};
use creeer_math::glam::{DQuat, DVec3};

/// A body as it sounds: what it is made of, its shape in its own axes (x along a bar), its mass
/// (kg), where its centre is and how its axes are turned in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body<'a> {
    pub matter: &'a Matter,
    pub shape: Shape,
    pub mass: f64,
    pub centre: DVec3,
    pub turn: DQuat,
}

/// The turn that brings a box's axes in `order` (`Shape::of_box`) to x, y and z.
fn axes(order: [usize; 3]) -> DQuat {
    let unit = [DVec3::X, DVec3::Y, DVec3::Z];
    let m = creeer_math::glam::DMat3::from_cols(unit[order[0]], unit[order[1]], unit[order[2]]);
    // An odd order mirrors: its third axis turned about to keep a rotation.
    if m.determinant() < 0.0 {
        let m =
            creeer_math::glam::DMat3::from_cols(unit[order[0]], unit[order[1]], -unit[order[2]]);
        DQuat::from_mat3(&m)
    } else {
        DQuat::from_mat3(&m)
    }
}

impl<'a> Body<'a> {
    /// A box of these half extents (m) at `centre`, turned by `rotation`, of `mass` kg (or as
    /// its material and size make it).
    pub fn boxed(
        matter: &'a Matter,
        half: DVec3,
        (centre, rotation): (DVec3, DQuat),
        mass: Option<f64>,
    ) -> Self {
        let (shape, order) = Shape::of_box(half.to_array());
        Self {
            matter,
            shape,
            mass: mass.unwrap_or(matter.acoustic.density * shape.volume()),
            centre,
            turn: rotation * axes(order),
        }
    }

    /// A round member (a log, a rod) of `radius` m from `a` to `b`.
    pub fn member(matter: &'a Matter, (a, b): (DVec3, DVec3), radius: f64) -> Self {
        let along = b - a;
        let length = along.length().max(1e-3);
        let shape = Shape::Bar {
            length,
            width: 2.0 * radius,
            depth: 2.0 * radius,
            round: true,
        };
        Self {
            matter,
            shape,
            mass: matter.acoustic.density * shape.volume(),
            centre: 0.5 * (a + b),
            turn: DQuat::from_rotation_arc(DVec3::X, along / length),
        }
    }

    /// The radius its surface meets things with (m): half a bar's thickness, a ball's.
    pub fn curvature(&self) -> f64 {
        match self.shape {
            Shape::Bar { width, depth, .. } => 0.5 * width.min(depth),
            Shape::Ball { radius } => radius,
        }
    }

    /// How it rings struck at `point` by `impulse` (world, N·s) for `contact` s.
    pub fn modes(&self, point: DVec3, impulse: DVec3, contact: f64) -> Modes {
        let local = self.turn.inverse() * impulse;
        let along = match self.shape {
            Shape::Bar { length, .. } => {
                let x = (self.turn.inverse() * (point - self.centre)).x;
                (x / length + 0.5).clamp(0.0, 1.0)
            }
            Shape::Ball { .. } => 0.0,
        };
        let blow = Blow {
            along,
            impulse: local.to_array(),
            contact,
        };
        bar(&self.matter.acoustic, &self.shape, self.mass, &blow, 0.0)
    }
}

/// The share of a hammer's blow that strikes across the nail and its own head, not along: no blow
/// by hand is square (an estimate).
const GLANCE: f64 = 0.15;

/// What a body met.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Other<'a> {
    Ground(&'a Ground),
    Body(Body<'a>),
}

/// The compliance of a ground's surface (Poisson's ratio taken as 0.3).
fn ground_compliance(g: &Ground) -> f64 {
    (1.0 - 0.09) / g.modulus
}

/// `a` met `other` at `point`, taking `impulse` (world, N·s): the sound of both ringing and the
/// click of the stop. `seed` makes it its own.
pub fn impact(
    a: &Body<'_>,
    other: &Other<'_>,
    point: DVec3,
    impulse: DVec3,
    seed: u64,
) -> VoiceSpec {
    let j = impulse.length();
    let (reduced, compliance, radius) = match other {
        Other::Ground(g) => (a.mass, ground_compliance(g), a.curvature()),
        Other::Body(b) => (
            a.mass * b.mass / (a.mass + b.mass),
            b.matter.acoustic.compliance(),
            a.curvature() * b.curvature() / (a.curvature() + b.curvature()),
        ),
    };
    let modulus = contact_modulus(a.matter.acoustic.compliance(), compliance);
    let tau = contact_time(reduced, radius, modulus, j / reduced);
    let mut modes = a.modes(point, impulse, tau);
    let (gain, corner) = click(a.shape.size());
    let mut stop = (gain * j / a.mass, corner);
    if let Other::Body(b) = other {
        modes.extend(&b.modes(point, -impulse, tau));
        let (gain, corner) = click(b.shape.size());
        if gain * j / b.mass > stop.0 {
            stop = (gain * j / b.mass, corner);
        }
    }
    spec(point, modes, stop, tau, seed)
}

/// A blow's sound: its modes rung once and its click.
#[allow(clippy::cast_possible_truncation)] // Pa, s and Hz as f32
fn spec(at: DVec3, modes: Modes, (gain, corner): (f64, f64), tau: f64, seed: u64) -> VoiceSpec {
    VoiceSpec {
        at,
        bus: Bus::Effects,
        key: 0,
        modes,
        click: Click {
            gain: gain as f32,
            contact: tau as f32,
            corner: corner as f32,
        },
        excite: Excite::Blow,
        peak: (modes.peak() + gain) as f32,
        seed,
    }
}

/// A tool's head of `head` (a body: its material, size and mass) meeting `target` at `point` at
/// `velocity` (m/s, world): its blow, rebounding as hard things do (restitution ½).
pub fn strike(
    head: &Body<'_>,
    target: &Other<'_>,
    point: DVec3,
    velocity: DVec3,
    seed: u64,
) -> VoiceSpec {
    let reduced = match target {
        Other::Ground(_) => head.mass,
        Other::Body(b) => head.mass * b.mass / (head.mass + b.mass),
    };
    impact(head, target, point, -1.5 * reduced * velocity, seed)
}

/// A fastener (its material, diameter and length: m) driven by `driver` into `part` at `point`
/// along `axis` (into the part): blow `blow` of the driver's blows. The fastener stands out less
/// with each blow, and rings higher (`modes::cantilever`); the last seats it on the wood.
pub fn driven(
    lib: &Library,
    driver: &Driver,
    (rod, diameter, length): (&Matter, f64, f64),
    part: &Body<'_>,
    (point, axis): (DVec3, DVec3),
    blow: u32,
    seed: u64,
) -> VoiceSpec {
    let Some(head_matter) = lib.matter(&driver.head) else {
        return spec(point, Modes::default(), (0.0, 1000.0), 1e-3, seed);
    };
    let blows = f64::from(driver.blows.max(1));
    let k = f64::from(blow.min(driver.blows - 1));
    // It stands out by what is left to drive: its whole length before the first blow.
    let standing = length * (1.0 - k / blows);
    let last = blow + 1 >= driver.blows;
    let half = DVec3::new(driver.size.0, driver.size.1, driver.size.2) * 0.5;
    let head = Body::boxed(
        head_matter,
        half,
        (
            point - axis * half.x,
            DQuat::from_rotation_arc(DVec3::X, axis),
        ),
        Some(driver.mass),
    );
    let speed = driver.speed;
    let j = 1.5 * driver.mass * speed;
    let steel = head_matter.acoustic.compliance();
    // Steel on the fastener's head; the last blow on the part's face as well.
    let compliance = if last {
        part.matter.acoustic.compliance()
    } else {
        steel
    };
    let tau = contact_time(
        driver.mass,
        (0.5 * diameter).max(0.002),
        contact_modulus(steel, compliance),
        speed,
    );
    // The head rings along its length and, by the blow's glancing share, across it.
    let mut modes = head.modes(
        point,
        -axis * j + axis.any_orthonormal_vector() * GLANCE * j,
        tau,
    );
    // The part is driven through the fastener as it goes in (over its travel this blow).
    let travel = (length / blows / speed).max(tau);
    modes.extend(&part.modes(point, axis * j, if last { tau } else { travel }));
    if !last {
        // The fastener's standing part, struck across by a glancing share of the blow.
        modes.extend(&cantilever(
            &rod.acoustic,
            (standing, diameter),
            GLANCE * j,
            speed,
            tau,
        ));
    }
    let (gain, corner) = click(head.shape.size());
    spec(point, modes, (gain * 1.5 * speed, corner), tau, seed)
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
