//! Planet gravity and ground contacts against analytic results: orbits, the Coriolis
//! deflection of a falling body, the weaker effective gravity at the equator, rolling and
//! sliding on a density-field ground, contact reduction for boxes, waking after the ground
//! changed.

use super::ground::Ground;
use super::shape::{Shape, StaticShape};
use super::*;
use crate::gravity::PlanetGravity;
use creeer_math::det;
use creeer_math::glam::DMat3;
use std::sync::atomic::{AtomicBool, Ordering};

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.806_65;
const R: f64 = 500_000.0;

fn home(spin: bool) -> PlanetGravity {
    let omega = std::f64::consts::TAU / 21_600.0;
    PlanetGravity {
        mu: 9.81 * R * R,
        spin: if spin { DVec3::Y * omega } else { DVec3::ZERO },
    }
}

/// Ground from a density function, with a numerical gradient.
struct FieldGround<F>(F);

impl<F> std::fmt::Debug for FieldGround<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FieldGround")
    }
}

impl<F: Fn(DVec3) -> f64 + Send + Sync> Ground for FieldGround<F> {
    fn density_gradient(&self, p: DVec3) -> (f64, DVec3) {
        let h = 1e-5;
        let d = &self.0;
        let g = DVec3::new(
            d(p + DVec3::X * h) - d(p - DVec3::X * h),
            d(p + DVec3::Y * h) - d(p - DVec3::Y * h),
            d(p + DVec3::Z * h) - d(p - DVec3::Z * h),
        ) / (2.0 * h);
        (d(p), g)
    }
}

fn ball(position: DVec3) -> Body {
    Body::rigid(Shape::Sphere { radius: 0.25 }, 10.0, position, DT)
}

#[test]
fn a_circular_orbit_keeps_its_radius_and_angular_speed() {
    // 20 km up, a quarter of an orbit (T = 2π·√(r³/μ) = 1504 s): Newmark is symplectic, so
    // the radius only oscillates, and the phase error is of order (ω·h)².
    let g = home(false);
    let r = R + 20_000.0;
    let speed = (g.mu / r).sqrt();
    let mut world = World::new(DT);
    world.planet = Some(g);
    let mut b = ball(DVec3::X * r);
    b.structure.velocity[0] = DVec3::Z * speed;
    world.add(b);
    let steps = 22_560; // 376 s
    let mut worst = 0.0f64;
    for _ in 0..steps {
        world.step();
        let x = world.bodies[0].structure.position[0];
        worst = worst.max((x.length() - r).abs());
    }
    assert!(worst < 1e-3, "radius changed by {worst} m");
    let x = world.bodies[0].structure.position[0];
    let angle = det::atan2(x.z, x.x);
    let expected = speed / r * f64::from(steps) * DT;
    assert!(
        (angle - expected).abs() < 1e-6,
        "angle {angle} vs {expected}"
    );
}

#[test]
fn a_falling_body_is_deflected_east_by_the_coriolis_force() {
    // Dropped from rest (in the rotating frame) at the equator: to first order in ω the
    // body lands ω·g·t³/3 east of the point below it (0.95 m after 10 s on a 6-hour day).
    let g = home(true);
    let start = DVec3::X * (R + 1000.0);
    let mut world = World::new(DT);
    world.planet = Some(g);
    world.add(ball(start));
    for _ in 0..600 {
        world.step();
    }
    let x = world.bodies[0].structure.position[0];
    let east = g.spin.cross(DVec3::X).normalize();
    let t: f64 = 10.0;
    let g_eff = g.acceleration(start).length();
    let expected = g.spin.length() * g_eff * t * t * t / 3.0;
    let deflection = (x - start).dot(east);
    assert!(
        (deflection / expected - 1.0).abs() < 0.02,
        "deflected {deflection} m east, expected {expected} m"
    );
}

#[test]
fn effective_gravity_is_weaker_at_the_equator() {
    // Fall in 2 s from rest: ½·g_eff·t², g_eff = μ/R² at the pole and μ/R² − ω²·R at the
    // equator (0.43 % less).
    let g = home(true);
    let fall = |start: DVec3| {
        let mut world = World::new(DT);
        world.planet = Some(g);
        world.add(ball(start));
        for _ in 0..120 {
            world.step();
        }
        start.length() - world.bodies[0].structure.position[0].length()
    };
    let pole = fall(DVec3::Y * R);
    let equator = fall(DVec3::X * R);
    let omega = g.spin.length();
    let want = (9.81 - omega * omega * R) / 9.81;
    assert!(
        (pole / (0.5 * 9.81 * 4.0) - 1.0).abs() < 1e-4,
        "pole fall {pole}"
    );
    assert!(
        (equator / pole - want).abs() < 2e-5,
        "ratio {}",
        equator / pole
    );
}

/// A tilted plane as a density field: identical to `StaticShape::Plane`.
fn ramp(degrees: f64) -> (Arc<dyn Ground>, DVec3, DVec3) {
    let (sin, cos) = det::sin_cos(degrees.to_radians());
    let normal = DVec3::new(-sin, cos, 0.0);
    let down = DVec3::new(-cos, -sin, 0.0);
    (
        Arc::new(FieldGround(move |p: DVec3| p.dot(normal))),
        normal,
        down,
    )
}

#[test]
fn a_ball_rolls_down_ground_at_five_sevenths_of_g_sin() {
    // Rolling without slipping (μ = 0.6 > 2/7·tan 15°): a = 5/7·g·sin θ.
    let (ground, normal, down) = ramp(15.0);
    let mut world = World::new(DT);
    world.ground = Some(ground);
    world.static_friction = 0.6;
    let mut b = ball(normal * 0.25);
    b.friction = 0.6;
    world.add(b);
    let along = |w: &World| w.bodies[0].structure.position[0].dot(down);
    for _ in 0..60 {
        world.step();
    }
    let start = along(&world);
    for _ in 0..60 {
        world.step();
    }
    let a = 5.0 / 7.0 * G * det::sin(15f64.to_radians());
    let want = 0.5 * a * 3.0;
    let rolled = along(&world) - start;
    assert!(
        (rolled / want - 1.0).abs() < 0.01,
        "rolled {rolled} m, expected {want} m"
    );
}

#[test]
fn a_box_slides_on_ground_exactly_as_on_a_plane() {
    // The F2 ramp (40°, μ = 0.3) with the ramp as ground instead of a plane: on flat ground the
    // box has the same four corner contacts, so the motion agrees to rounding.
    let run = |as_ground: bool| {
        let (ground, normal, down) = ramp(40.0);
        let mut world = World::new(DT);
        if as_ground {
            world.ground = Some(ground);
        } else {
            world.statics.push(StaticShape::Plane {
                normal,
                offset: 0.0,
            });
        }
        world.static_friction = 0.3;
        let mut block = Body::rigid(
            Shape::Box {
                half: DVec3::splat(0.5),
            },
            100.0,
            normal * 0.5,
            DT,
        );
        block.structure.orientation[0] = rotation(DVec3::Z, 40f64.to_radians());
        block.friction = 0.3;
        world.add(block);
        let mut contacts = 0;
        for _ in 0..120 {
            world.step();
            contacts = contacts.max(world.contacts.len());
        }
        (world.bodies[0].structure.position[0].dot(down), contacts)
    };
    let (plane, plane_contacts) = run(false);
    let (ground, ground_contacts) = run(true);
    assert_eq!(
        ground_contacts, plane_contacts,
        "contact reduction keeps the corners only"
    );
    assert!(
        (plane - ground).abs() < 1e-6,
        "plane {plane} m, ground {ground} m"
    );
}

#[test]
fn a_box_resting_on_a_bump_touches_the_bump() {
    // A 2 m crate dropped onto flat ground with a 15 cm rounded bump under its middle rests
    // on the bump, never sinking into it.
    let bump = |p: DVec3| p.y - 0.15 * det::exp(-(p.x * p.x + p.z * p.z) / (2.0 * 0.15 * 0.15));
    let mut world = World::new(DT);
    world.ground = Some(Arc::new(FieldGround(bump)));
    world.add(Body::rigid(
        Shape::Box {
            half: DVec3::splat(1.0),
        },
        500.0,
        DVec3::Y * 1.3,
        DT,
    ));
    let mut touched_inside_a_face = false;
    for _ in 0..240 {
        world.step();
        // A contact whose point on the box is not one of its corners: the bump.
        touched_inside_a_face |= world.contacts.iter().any(|c| match c.a {
            super::contact::Side::Node { local, .. } => local.abs().min_element() < 0.99,
            super::contact::Side::Fixed(_) => false,
        });
    }
    assert!(
        touched_inside_a_face,
        "the bump never touched the bottom face"
    );
    // No point of the bottom face is inside the ground by more than the contact compression.
    let s = &world.bodies[0].structure;
    let r = DMat3::from_quat(s.orientation[0]);
    let mut deepest = 0.0f64;
    for i in 0..=40 {
        for k in 0..=40 {
            let local = DVec3::new(f64::from(i) / 20.0 - 1.0, -1.0, f64::from(k) / 20.0 - 1.0);
            deepest = deepest.min(bump(s.position[0] + r * local));
        }
    }
    assert!(
        deepest > -0.005,
        "the ground pokes {} m into the box",
        -deepest
    );
    assert!(world.bodies[0].sleep.asleep, "comes to rest");
}

/// Ground that can be lowered by 2 m (stands for a terrain edit).
#[derive(Debug, Default)]
struct Lowerable(AtomicBool);

impl Ground for Lowerable {
    fn density_gradient(&self, p: DVec3) -> (f64, DVec3) {
        let depth = if self.0.load(Ordering::Relaxed) {
            2.0
        } else {
            0.0
        };
        (p.y + depth, DVec3::Y)
    }
}

#[test]
fn a_sleeping_body_wakes_and_falls_when_the_ground_is_removed() {
    let ground = Arc::new(Lowerable::default());
    let mut world = World::new(DT);
    world.ground = Some(ground.clone());
    world.add(ball(DVec3::Y * 0.25));
    for _ in 0..240 {
        world.step();
    }
    assert!(world.bodies[0].sleep.asleep, "rests and sleeps");
    ground.0.store(true, Ordering::Relaxed);
    world.wake_near(DVec3::ZERO, 5.0);
    for _ in 0..60 {
        world.step();
    }
    let y = world.bodies[0].structure.position[0].y;
    assert!(
        (y - (0.25 - 2.0)).abs() < 1e-3,
        "fell onto the lowered ground: y = {y}"
    );
}

#[test]
fn a_ball_rests_on_a_planet_sized_sphere() {
    // Without rotation the sphere is level everywhere: the ball sinks in by m·g/k and stops.
    let g = home(false);
    let up = DVec3::new(0.3, 0.8, -0.52).normalize();
    let mut world = World::new(DT);
    world.planet = Some(g);
    world.ground = Some(Arc::new(FieldGround(|p: DVec3| p.length() - R)));
    world.add(ball(up * (R + 0.4)));
    for _ in 0..240 {
        world.step();
    }
    let s = &world.bodies[0].structure;
    let sink = 0.25 - (s.position[0].length() - R);
    let want = 10.0 * 9.81 / world.contact_stiffness;
    assert!(
        (sink / want - 1.0).abs() < 0.02,
        "sank {sink:e} m, expected {want:e}"
    );
    assert!(s.velocity[0].length() < 1e-6 && world.bodies[0].sleep.asleep);
}
