//! A player's character (G1.2, ADR-0023): a person in a work suit, as a physics body like
//! any other. It replaces F5's test ball (`avatar.rs`).
//!
//! - **The body** is one rigid body: torso and head as a box from the knees up, with the mass and inertia of a person in a suit.
//! - **The legs** are a spring and damper from the hip to the ground found below it (a ray),
//!   with a person's leg stiffness and a strength limit: they hold the body up, absorb a
//!   landing, step it up a kerb, and push whatever they stand on back. Compressed beyond their
//!   reach, the body itself meets the ground: a real impact, its deceleration the contacts'.
//! - **Walking** is a force at the feet, within the friction between boot and ground (μ times
//!   the legs' force: a slope beyond the friction angle cannot be climbed, ice is slippery),
//!   and within the legs' strength. Its reaction pushes the ground's body.
//! - **Balance** is a torque towards upright, within the body's strength: a hard enough
//!   shove topples it. In the air there is neither (no air control: the jetpack's thrusters
//!   are the way to steer there, G1.2).
//!
//! The server steers every character, and a client its own predicted one, with the same code
//! from the same inputs (ADR-0008). A step's reactions on other bodies are loads for that step
//! alone: [`release`] takes them off after it.

use creeer_math::glam::{DQuat, DVec3};
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World};

/// A person (80 kg) in a work suit with its backpack (22 kg), without the jetpack's gas.
pub const DRY: f64 = 102.0;
/// The jetpack's nitrogen, full (kg): two 6 l bottles at 300 bar.
pub const TANK: f64 = 3.0;
/// The whole, the tank full.
pub const MASS: f64 = DRY + TANK;
/// Cold nitrogen's specific impulse (s): NASA's MMU and SAFER packs, about 70.
pub const ISP: f64 = 70.0;
/// The jetpack's most thrust (N): a little over its wearer's weight, for a second or two.
pub const THRUST_MOST: f64 = 1500.0;
/// Its attitude jets: their damping of spin (N·m·s/rad), most torque (N·m), and lever (m).
const ATTITUDE: (f64, f64, f64) = (60.0, 30.0, 0.3);
/// Standard gravity (m/s²), of the specific impulse.
const G0: f64 = 9.806_65;
/// From the feet to the top of the helmet, standing (m).
pub const HEIGHT: f64 = 1.85;
/// The hip's height above the feet, standing (m).
const HIP: f64 = 0.92;
/// The leg spring's length unloaded (m): standing it is pressed by the body's weight, 3.4 cm.
const LEG_LENGTH: f64 = HIP + MASS * 9.806_65 / LEG_STIFFNESS;
/// How far below the hip the legs reach for the ground (m): a step down they take.
const LEG_REACH: f64 = 1.1;
/// Leg stiffness (N/m): a person's in hopping and running, about 20 kN/m for 70 kg (Farley
/// & Morgenroth 1999), with this mass.
pub const LEG_STIFFNESS: f64 = 30_000.0;
/// Its damping, 0.6 of critical (a landing settles in a bounce or so).
const LEG_DAMPING: f64 = 1.2 * 1774.8;
/// The most the legs push (N): about 3.5 times the body's weight.
pub const LEG_MOST: f64 = 3600.0;
/// The most the legs push sideways for walking (N): about 4 m/s² of this mass.
const STRIDE_MOST: f64 = 420.0;
/// The time a walk closes the difference to the velocity asked for in (s).
const RESPONSE: f64 = 0.25;
/// The fastest it may be asked to move (m/s: a run in a suit).
pub const MOST_SPEED: f64 = 4.0;
/// Boot soles' friction (rubber on rock, dry).
const BOOTS: f64 = 0.9;
/// Balance: stiffness and damping of the torque towards upright (N·m/rad, N·m·s/rad), and
/// the most of it (N·m): a strong person's ankles and hips (beyond, it topples).
const BALANCE: (f64, f64, f64) = (5000.0, 640.0, 600.0);
/// Damping of spin about the vertical (N·m·s/rad): the feet hold it.
const YAW_DAMPING: f64 = 200.0;
/// How fast it leaves the ground in a jump (m/s: 0.4 m high).
pub const JUMP_SPEED: f64 = 2.8;
/// A swimmer's thrust at most (N): a person's, about 60–100 N sprinting.
const SWIM_MOST: f64 = 80.0;
/// The torque a swimmer keeps upright with at most (N·m).
const SWIM_BALANCE: f64 = 100.0;
/// How far above the surface its centre may be and it still swim (m): its body in the water.
const SWIMS_ABOVE: f64 = 0.4;

/// The body's box: from its knees (0.5 m above the feet) to the top of its helmet, as wide
/// and deep as a person in a suit (half extents, m). A box, not a rounded hull: two
/// 40-cornered hulls cost SAT 6,400 edge pairs, and a crowd of eight among crates took
/// 1.9 ms a tick to find its contacts; a body that does not turn about the vertical loses
/// little by being boxy.
const TORSO: DVec3 = DVec3::new(0.24, 0.5 * (HEIGHT - KNEES), 0.16);
/// Its bottom's height above the feet, standing (m).
const KNEES: f64 = 0.5;

/// Where the body's centre lies above the feet, standing (m).
const CENTRE: f64 = 0.5 * (HEIGHT + KNEES);

/// The hip's offset from the body's centre, in its frame (m, along its up).
fn hip() -> f64 {
    HIP - CENTRE
}

/// Where the body's centre lies above its feet, standing, and its hip (m): for drawing it.
pub fn heights() -> (f64, f64) {
    (CENTRE, HIP)
}

/// The body's half extents (m): for drawing it.
pub fn torso() -> DVec3 {
    TORSO
}

/// The character's body standing with its feet at `feet`, upright along `up`, at rest.
pub fn body(feet: DVec3, up: DVec3, dt: f64) -> Body {
    let at = feet + up.normalize() * CENTRE;
    let mut body = Body::rigid(Shape::Box { half: TORSO }, MASS, at, dt);
    let turn = DQuat::from_rotation_arc(DVec3::Y, up.normalize());
    body.structure.orientation[0] = turn;
    body.friction = BOOTS;
    body.restitution = 0.0;
    body.solver.reset_history(&body.structure);
    body
}

/// A force (and torque) put on another body for one step: taken off by [`release`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reaction {
    pub body: usize,
    pub node: usize,
    pub force: DVec3,
    pub torque: DVec3,
    /// Whether the body was stepped as held (`Body::held`) before it was pushed.
    pub held: bool,
    /// The body's mass after the step (a jetpack's gas spent in it), if it changes.
    pub mass: Option<f64>,
}

/// What a player asks of their character for one step.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Intent {
    /// Where to walk (m/s along the ground; or swim).
    pub walk: DVec3,
    /// Jump if standing (swim up, in water).
    pub jump: bool,
    /// The jetpack's push, as a fraction of its most (world frame; length at most 1).
    pub thrust: DVec3,
}

impl Intent {
    /// Walking (or jumping), no thrust.
    pub fn walk(walk: DVec3, jump: bool) -> Self {
        Self {
            walk,
            jump,
            thrust: DVec3::ZERO,
        }
    }
}

/// The gas left in a character's jetpack (kg).
pub fn gas(body: &Body) -> f64 {
    (body.structure.masses()[0] - DRY).max(0.0)
}

/// Fills a character's jetpack (at its pod, G1.6).
pub fn refill(body: &mut Body) {
    body.solver.set_mass(&mut body.structure, 0, MASS);
}

/// How a character stands after [`steer`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Footing {
    /// The legs' force on the body (N, world; zero in the air).
    pub legs: DVec3,
    /// The ground under the feet (its normal), if the legs reach it.
    pub ground: Option<DVec3>,
}

/// Steers character `index` for the next step: towards `walk` (m/s along the ground; `up`
/// the local vertical), jumping if asked and standing (swimming up, in water). `water` is
/// how deep its centre is under the sea's surface (m; none: no sea). Its reactions on the
/// bodies it stands on go into `reactions` (take them off after the step: [`release`]).
pub fn steer(
    world: &mut World,
    index: usize,
    intent: Intent,
    (up, water): (DVec3, Option<f64>),
    reactions: &mut Vec<Reaction>,
) -> Footing {
    let (walk, jump) = (intent.walk, intent.jump);
    let (x, q, v, w) = {
        let s = &world.bodies[index].structure;
        (
            s.position[0],
            s.orientation[0],
            s.velocity[0],
            s.angular_velocity[0],
        )
    };
    let hip = x + up * hip();
    let hit = world.ray(hip, -up, LEG_REACH, Some(index));
    // Asleep and asked for nothing: it stands as it fell asleep. (Its legs' force taken
    // again from where it rests differs from the force it fell asleep under in its last
    // bits, and a changed load wakes a body: it never slept, and was never parked.)
    if world.bodies[index].sleep.asleep && intent == Intent::default() {
        return Footing {
            legs: world.bodies[index].structure.load[0],
            ground: hit.map(|h| h.normal),
        };
    }
    let mut footing = Footing::default();
    let mut mine = None;
    let (mut force, mut torque) = (DVec3::ZERO, DVec3::ZERO);
    if let Some(hit) = hit.filter(|h| h.normal.dot(up) > 0.0) {
        // The ground's motion at the foot (a raft, a crate, a lift).
        let ground_v = hit.body.map_or(DVec3::ZERO, |(b, n)| {
            let s = &world.bodies[b].structure;
            s.velocity[n] + s.angular_velocity[n].cross(hit.point - s.position[n])
        });
        let closing = (v - ground_v).dot(up);
        let legs = (LEG_STIFFNESS * (LEG_LENGTH - hit.distance) - LEG_DAMPING * closing)
            .clamp(0.0, LEG_MOST);
        // Walking: towards the velocity asked for, along the ground, relative to it.
        let n = hit.normal;
        let along = |u: DVec3| u - up * u.dot(up);
        let mut wanted = along(walk);
        if wanted.length() > MOST_SPEED {
            wanted = wanted.normalize() * MOST_SPEED;
        }
        let mut stride = (wanted - along(v - ground_v)) * (MASS / RESPONSE);
        if stride.length() > STRIDE_MOST {
            stride = stride.normalize() * STRIDE_MOST;
        }
        // The foot's force within the friction cone of the ground under it.
        let mu = BOOTS.min(
            hit.body
                .map_or(world.static_friction, |(b, _)| world.bodies[b].friction),
        );
        let foot = up * legs + stride;
        let pressing = foot.dot(n).max(0.0);
        let mut sliding = foot - n * foot.dot(n);
        if sliding.length() > mu * pressing {
            sliding = sliding.normalize_or_zero() * mu * pressing;
        }
        force = n * pressing + sliding;
        // Upright, within the body's strength; and the feet hold its spin about the vertical.
        let tilt = (q * DVec3::Y).cross(up);
        let (k, c, most) = BALANCE;
        let spin_up = up * w.dot(up);
        torque = tilt * k - (w - spin_up) * c - spin_up * YAW_DAMPING;
        if torque.length() > most {
            torque = torque.normalize() * most;
        }
        if let Some((b, node)) = hit.body {
            let s = &world.bodies[b].structure;
            mine = Some(Reaction {
                body: b,
                node,
                force: -force,
                torque: -torque - (hit.point - s.position[node]).cross(force),
                held: world.bodies[b].held,
                mass: None,
            });
        }
        footing = Footing {
            legs: force,
            ground: Some(n),
        };
        if jump && v.dot(up) < 0.5 * JUMP_SPEED && legs > 0.0 {
            let impulse = up * (MASS * (JUMP_SPEED - v.dot(up).max(0.0)));
            world.apply_impulse(index, 0, impulse, x);
            if let Some((other, node)) = hit.body {
                world.apply_impulse(other, node, -impulse, hit.point);
            }
        }
    } else if water.is_some_and(|depth| depth > -SWIMS_ABOVE) {
        // Swimming: towards the velocity asked for, up while asked to rise, within a
        // swimmer's thrust; the sea's buoyancy and drag are its own (`surroundings.rs`).
        let along = |u: DVec3| u - up * u.dot(up);
        let mut stroke = (along(walk) - along(v)) * (MASS / RESPONSE);
        if jump {
            stroke += up * SWIM_MOST;
        }
        if stroke.length() > SWIM_MOST {
            stroke = stroke.normalize() * SWIM_MOST;
        }
        force = stroke;
        // A swimmer keeps upright too, more weakly.
        let tilt = (q * DVec3::Y).cross(up);
        let (k, c, _) = BALANCE;
        torque = tilt * k - w * c;
        if torque.length() > SWIM_BALANCE {
            torque = torque.normalize() * SWIM_BALANCE;
        }
    }
    if let Some((push, hold, mass)) = jetpack(world, index, intent.thrust, w) {
        force += push;
        torque += hold;
        reactions.push(Reaction {
            body: index,
            node: 0,
            force: DVec3::ZERO,
            torque: DVec3::ZERO,
            held: true,
            mass: Some(mass),
        });
    }
    let s = &mut world.bodies[index].structure;
    s.load[0] = force;
    s.torque[0] = torque;
    // Standing, it is stepped as held (BDF2), and so is another body its legs push: equal and
    // opposite loads keep momentum only under one scheme, and a client's prediction, set back to
    // the server's state, cannot know Newmark's carried acceleration (8 % of its corrections
    // were over a centimetre stepped free on the ground). In the air it steps free (Newmark),
    // whose impulses are exact: BDF2 takes part of a force switched on with it (a full tank
    // burnt held fell 0.6 % short of Tsiolkovsky's Δv; free, it is exact to six digits).
    world.bodies[index].held = footing.ground.is_some();
    if let Some(r) = mine.filter(|r: &Reaction| r.body != index) {
        // Stepped as the character is: equal and opposite loads keep momentum only so.
        world.bodies[r.body].held = true;
        let s = &mut world.bodies[r.body].structure;
        s.load[r.node] += r.force;
        s.torque[r.node] += r.torque;
        reactions.push(r);
    }
    footing
}

/// The jetpack for a step: its push (`thrust` as a fraction of its most, while gas lasts)
/// and its attitude jets' hold on the spin `w`, and the body's mass after the step. The gas
/// they spend is the push over its exhaust's speed (Isp·g₀); the body steps with half the
/// step's gas spent (the mean mass over the step: Tsiolkovsky's Δv to second order) and ends
/// it without the step's.
fn jetpack(
    world: &mut World,
    index: usize,
    thrust: DVec3,
    w: DVec3,
) -> Option<(DVec3, DVec3, f64)> {
    let body = &mut world.bodies[index];
    let (mass, gas) = (body.structure.masses()[0], gas(body));
    if thrust == DVec3::ZERO || gas <= 0.0 {
        return None;
    }
    let exhaust = ISP * G0;
    let (damping, most, arm) = ATTITUDE;
    let mut push = thrust.clamp_length_max(1.0) * THRUST_MOST;
    let mut hold = (-w * damping).clamp_length_max(most);
    // Kilograms a step: the push's, and the attitude jets' (a pair at their lever).
    let dt = world.dt();
    let mut burn = (push.length() + hold.length() / arm) / exhaust * dt;
    if burn > gas {
        (push, hold) = (push * (gas / burn), hold * (gas / burn));
        burn = gas;
    }
    let body = &mut world.bodies[index];
    body.solver
        .set_mass(&mut body.structure, 0, mass - 0.5 * burn);
    Some((push, hold, mass - burn))
}

/// Takes the step's reactions off the bodies they were put on.
pub fn release(world: &mut World, reactions: &mut Vec<Reaction>) {
    for r in reactions.drain(..) {
        if let Some(body) = world.bodies.get_mut(r.body) {
            body.take_load(r.node, r.force);
            body.structure.torque[r.node] -= r.torque;
            body.held = r.held;
            if let Some(mass) = r.mass {
                body.solver.set_mass(&mut body.structure, r.node, mass);
            }
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod fall_tests;
/// Injury from impacts.
pub mod injury;
#[cfg(test)]
mod jet_tests;
/// A character fallen: its ragdoll.
pub mod ragdoll;
#[cfg(test)]
mod swim_tests;
