//! Ships and loose objects: in the physics bubble, on rails, or resting on a body.
//!
//! Leaving the bubble (too far from the focus, or for time warp) a vessel is **packed**:
//! a body at rest on the ground is stored in that body's fixed frame (it stays put while the
//! body turns and moves), anything else goes on rails as one particle at its centre of mass
//! with its full mass and gravity. Its shape and spin are kept in a frame centred on its
//! centre of mass with inertial axes; while on rails it turns at a constant angular velocity
//! (exact for spin about a principal axis; a tumbling brick's precession is not followed).

use super::bodies::Bodies;
use super::bubble::Bubble;
use super::pose::Pose;
use super::terrains::BodyRef;
use creeer_math::glam::{DQuat, DVec3};
use creeer_orbit::G;
use creeer_orbit::rails::{Group, Particle};
use creeer_physics::world::Body;
use creeer_physics::world::frame::FrameChange;

/// Where a vessel is.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // one per vessel, rarely moved
pub enum Where {
    /// In the bubble's world.
    Bubble,
    /// On rails: `body` is kept in its centre-of-mass frame (inertial axes) as of `since`.
    Rails {
        /// Its centre of mass.
        group: Group,
        /// Its shape and spin about the centre of mass.
        body: Body,
        /// Time the shape was stored.
        since: f64,
    },
    /// Resting on a body: `body` in that body's fixed frame.
    Landed {
        /// The body it rests on.
        on: BodyRef,
        /// Its state in the body's frame.
        body: Body,
    },
}

/// A ship or a loose object.
#[derive(Clone, Debug)]
pub struct Vessel {
    /// Stable identifier.
    pub id: u64,
    /// Display name.
    pub name: String,
    /// Where it is.
    pub place: Where,
}

/// Mass-weighted centre of a body's nodes and their momentum-weighted velocity.
pub fn centre_of_mass(body: &Body) -> (DVec3, DVec3, f64) {
    let s = &body.structure;
    let (mut x, mut v, mut m) = (DVec3::ZERO, DVec3::ZERO, 0.0);
    for (i, &mass) in s.masses().iter().enumerate() {
        x += s.position[i] * mass;
        v += s.velocity[i] * mass;
        m += mass;
    }
    (x / m, v / m, m)
}

/// Takes body `index` out of the bubble (frame `pose` at time `t`) and stores it: landed if it
/// rests on the ground of a surface frame, on rails otherwise.
pub fn pack(bubble: &mut Bubble, pose: &Pose, index: usize, t: f64) -> Where {
    let mut body = bubble.world.remove_body(index);
    bubble.vessels.remove(index);
    if let Some(on) = bubble.anchor()
        && body.sleep.asleep
    {
        return Where::Landed { on, body };
    }
    let (x, v, mass) = centre_of_mass(&body);
    let (xi, vi) = pose.to_inertial(x, v);
    let centred = Pose::inertial(xi, vi);
    body.change_frame(&pose.change_to(&centred));
    Where::Rails {
        group: Group::new(t, &[(xi, vi)], vec![Particle { mass, mu: G * mass }]),
        body,
        since: t,
    }
}

/// Puts a stored vessel into the bubble (frame `pose` at time `t`); returns its body.
pub fn unpack(place: Where, bodies: &Bodies, pose: &Pose, t: f64) -> Body {
    match place {
        Where::Bubble => unreachable!("already in the bubble"),
        Where::Rails {
            group,
            mut body,
            since,
        } => {
            // Turn it by its own spin over the time on rails (about its centre of mass).
            let (_, w, _) = spin(&body);
            let angle = w.length() * (t - since);
            if angle > 0.0 {
                body.change_frame(&FrameChange {
                    rotation: DQuat::from_axis_angle(w.normalize(), angle),
                    translation: DVec3::ZERO,
                    velocity: DVec3::ZERO,
                    spin: DVec3::ZERO,
                });
            }
            let (x, v) = group.state(0);
            body.change_frame(&Pose::inertial(x, v).change_to(pose));
            body
        }
        Where::Landed { on, mut body } => {
            let frame = bodies.pose(on, t);
            body.change_frame(&frame.change_to(pose));
            body
        }
    }
}

/// Orientation, angular velocity and mass of a body's first node (its spin as a whole for a
/// rigid body).
fn spin(body: &Body) -> (DQuat, DVec3, f64) {
    let s = &body.structure;
    (s.orientation[0], s.angular_velocity[0], s.total_mass())
}

/// Inertial position of a stored vessel at time `t` (rails: its centre of mass).
pub fn stored_position(place: &Where, bodies: &Bodies, t: f64) -> Option<DVec3> {
    match place {
        Where::Bubble => None,
        Where::Rails { group, .. } => Some(group.state(0).0),
        Where::Landed { on, body } => {
            let (x, v, _) = centre_of_mass(body);
            Some(bodies.pose(*on, t).to_inertial(x, v).0)
        }
    }
}
