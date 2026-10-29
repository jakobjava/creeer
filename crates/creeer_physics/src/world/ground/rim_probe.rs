//! A probe (ignored): a pole leaning on a narrow hole's rim, as `creeer_server`'s `rim_tests.rs`
//! sets it, in the physics alone, over a field dug as the terrain digs (G2.10): `LEAN` (°),
//! `TICKS` and `HZ` from the environment. It found the friction anchors loaded beyond the stick
//! distance (ADR-0005, G2.10; `docs/adr/0005/anchors-kept-and-clamped.diff`) and shows the 10°
//! bottom cycle.

use crate::world::hull::Hull;
use crate::world::planet_tests::FieldGround;
use crate::world::shape::Shape;
use crate::world::*;
use creeer_math::det;
use std::sync::Arc;

fn dt() -> f64 {
    1.0 / std::env::var("HZ")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60.0)
}

fn capsule(p: DVec3, a: DVec3, b: DVec3, radius: f64) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
    (p - (a + ab * t)).length() - radius
}

fn dig(d: f64, brush: f64, k: f64) -> f64 {
    let h = (k - (-d - brush).abs()).max(0.0) / k;
    -((-d).min(brush) - 0.25 * k * h * h)
}

fn holed(p: DVec3) -> f64 {
    let hole = capsule(p, DVec3::ZERO, DVec3::new(0.0, -0.7, 0.0), 0.15);
    dig(p.y, hole, 0.15)
}

fn pole_hull() -> Arc<Hull> {
    let ring: Vec<DVec3> = (0..16)
        .map(|k| {
            let (s, c) = det::sin_cos(std::f64::consts::TAU * f64::from(k) / 16.0);
            DVec3::new(0.0, 0.06 * c, 0.06 * s)
        })
        .collect();
    let corners: Vec<DVec3> = ring
        .iter()
        .flat_map(|&r| [r + DVec3::X * 1.5, r - DVec3::X * 1.5])
        .collect();
    Arc::new(Hull::new(&corners).expect("a prism").0)
}

#[test]
#[ignore = "probe"]
fn trace_a_pole_on_the_rim() {
    let lean: f64 = std::env::var("LEAN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(20.0);
    let ticks: u32 = std::env::var("TICKS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(240);
    let hull = pole_hull();
    let mass = 800.0 * hull.volume();
    let mut world = World::new(dt());
    world.ground = Some(Arc::new(FieldGround(holed)));
    world.static_friction = 0.7;
    let (up, east) = (DVec3::Y, DVec3::X);
    let (s, c) = det::sin_cos(lean.to_radians());
    let axis = (up * c - east * s).normalize();
    let foot = -up * 0.3 + east * 0.08;
    let mut pole = Body::rigid(Shape::Hull(hull), mass, foot + axis * 1.5, dt());
    pole.structure.orientation[0] = DQuat::from_rotation_arc(DVec3::X, -axis);
    pole.surface = surface::Surface::new(9.0e9, 0.3);
    pole.friction = std::env::var("MU")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.6);
    world.add(pole);
    let start = world.energy();
    for t in 0..ticks {
        world.step();
        let s = &world.bodies[0].structure;
        let a = s.orientation[0] * DVec3::X;
        let foot = s.position[0] + a * 1.5;
        if t % 10 == 0 || t + 12 > ticks {
            println!(
                "{t}: v {:.4}, w {:.4} (about its axis {:+.4}), energy {:+.4} J, foot {:.4} down, \
                 tilt {:.2}°, fallbacks {}",
                s.velocity[0].length(),
                s.angular_velocity[0].length(),
                s.angular_velocity[0].dot(a),
                world.energy() - start,
                -foot.y,
                det::acos(a.dot(up).abs()).to_degrees(),
                world.bodies[0].solver.fallbacks,
            );
            if t + 4 > ticks {
                for c in world.touching(0, 0.002) {
                    println!(
                        "    touches at {:.3?} (r {:.3} from the axis), normal {:.3?}, gap {:.5}",
                        c.point,
                        DVec3::new(c.point.x, 0.0, c.point.z).length(),
                        c.normal,
                        c.gap
                    );
                }
            }
        }
    }
}
