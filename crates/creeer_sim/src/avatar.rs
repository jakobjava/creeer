//! A player's avatar: for now a ball the player rolls about, the test avatar of F5's
//! prediction (plan.md). It is a body of the physics world like any other: the player only
//! pushes it, with a force at its centre, and the world does the rest (it rolls, it is
//! stopped by walls and pushed by crates, friction and slopes act on it as on a crate).
//!
//! The push follows a velocity asked for along the ground: it closes the difference in
//! [`RESPONSE`], at most [`GRIP`] while standing on something (less in the air,
//! [`AIR_GRIP`]). A jump is a push off the ground, leaving it at [`JUMP_SPEED`]. The server
//! steers every avatar with it, and a client its own predicted one, from the same inputs.

use creeer_math::glam::DVec3;
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, Touch, World};

/// The avatar's radius (m).
pub const RADIUS: f64 = 0.4;
/// Its mass (kg).
pub const MASS: f64 = 80.0;
/// The fastest it may be asked to roll (m/s).
pub const MOST_SPEED: f64 = 7.0;
/// The time it closes the difference to the velocity asked for in (s).
pub const RESPONSE: f64 = 0.15;
/// Its most acceleration while standing (m/s²).
pub const GRIP: f64 = 9.0;
/// And in the air.
pub const AIR_GRIP: f64 = 1.5;
/// How fast it leaves the ground in a jump (m/s: about a metre high).
pub const JUMP_SPEED: f64 = 4.5;
/// The steepest ground it stands on (cosine of 50° from the vertical).
pub const STEEPEST: f64 = 0.643;

/// An avatar asked for nothing is braked until it spins slower than this (rad/s, and rolls
/// slower than this times its radius): the world's own threshold for stillness, so it
/// then falls asleep (a ball rolls on for ever on level ground).
const STILL: f64 = 1e-3;

/// A rolling solid ball's inertia about its contact (I/(m·r²) + 1): a force at its centre
/// accelerates it this many times less than it would slide it.
const ROLLING: f64 = 1.4;

/// The avatar's body at `at`, at rest (index-free: add it to a world).
pub fn body(at: DVec3, dt: f64) -> Body {
    let mut body = Body::rigid(Shape::Sphere { radius: RADIUS }, MASS, at, dt);
    body.friction = 0.9;
    body.restitution = 0.0;
    body
}

/// How near the ground counts as on it (m).
pub const ON_GROUND: f64 = 0.01;

/// The ground under body `index`: the most upright of what it touches now (within
/// [`ON_GROUND`]), if it is not steeper than [`STEEPEST`]. From the state alone, so the server
/// and a client predicting from the server's state find the same.
pub fn ground(world: &World, index: usize, up: DVec3) -> Option<Touch> {
    world
        .touching(index, ON_GROUND)
        .into_iter()
        .filter(|t| t.normal.dot(up) >= STEEPEST)
        .max_by(|a, b| a.normal.dot(up).total_cmp(&b.normal.dot(up)))
}

/// Steers avatar `index` of `world` for the next step: towards `walk` (m/s, its part along
/// the ground; `up` the local vertical), jumping if asked and standing. Returns whether it
/// stands on something.
///
/// A jump is the push of the legs, over in less than a step: an impulse, which leaves the
/// avatar rising at [`JUMP_SPEED`] and pushes what it stood on the other way (a crate it
/// jumps off is pressed down by as much).
pub fn steer(world: &mut World, index: usize, (walk, jump): (DVec3, bool), up: DVec3) -> bool {
    let ground = ground(world, index, up);
    let s = &world.bodies[index].structure;
    let n = ground.map_or(up, |g| g.normal);
    let along = |v: DVec3| v - n * v.dot(n);
    let mut wanted = along(walk);
    if wanted.length() > MOST_SPEED {
        wanted = wanted.normalize() * MOST_SPEED;
    }
    let mut accel = (wanted - along(s.velocity[0])) / RESPONSE;
    let most = if ground.is_some() { GRIP } else { AIR_GRIP };
    if accel.length() > most {
        accel = accel.normalize() * most;
    }
    let mut push = accel * MASS * if ground.is_some() { ROLLING } else { 1.0 };
    // An avatar left standing (asked for nothing, all but still) is not pushed: the world
    // may put it to sleep (any load would keep waking it).
    let still =
        along(s.velocity[0]).length() < STILL * RADIUS && s.angular_velocity[0].length() < STILL;
    if wanted == DVec3::ZERO && ground.is_some() && still {
        push = DVec3::ZERO;
    }
    let rising = s.velocity[0].dot(up);
    let centre = s.position[0];
    world.bodies[index].structure.load[0] = push;
    if let Some(g) = ground.filter(|_| jump && rising < 0.5 * JUMP_SPEED) {
        let impulse = up * (MASS * (JUMP_SPEED - rising.max(0.0)));
        world.apply_impulse(index, 0, impulse, centre);
        if let Some((other, node)) = g.other {
            world.apply_impulse(other, node, -impulse, g.point);
        }
    }
    ground.is_some()
}

#[cfg(test)]
mod tests;
