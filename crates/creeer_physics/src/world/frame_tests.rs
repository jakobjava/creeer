use super::*;
use crate::world::shape::Shape;
use creeer_math::det;

const DT: f64 = 1.0 / 60.0;
const R: f64 = 500_000.0;
const MU: f64 = 9.81 * R * R;

fn spinning_box(position: DVec3, velocity: DVec3) -> Body {
    let half = DVec3::new(0.5, 1.0, 1.5);
    let mut body = Body::rigid(Shape::Box { half }, 500.0, position, DT);
    let s = &mut body.structure;
    s.velocity[0] = velocity;
    s.angular_velocity[0] = DVec3::new(0.3, -0.2, 0.5);
    s.orientation[0] = DQuat::from_rotation_x(0.4) * DQuat::from_rotation_y(-1.1);
    body
}

#[test]
fn a_change_and_its_inverse_leave_every_state_unchanged() {
    let change = FrameChange {
        rotation: DQuat::from_axis_angle(DVec3::new(1.0, 2.0, -0.5).normalize(), 0.7),
        translation: DVec3::new(1e5, -3e4, 2e3),
        velocity: DVec3::new(10.0, -200.0, 35.0),
        spin: DVec3::new(1e-3, -4e-4, 2e-4),
    };
    let mut body = spinning_box(DVec3::new(5.0e5, 1e3, -2e3), DVec3::new(1.0, 2.0, 3.0));
    let before = body.structure.clone();
    body.change_frame(&change);
    body.change_frame(&change.inverse());
    let s = &body.structure;
    assert!((s.position[0] - before.position[0]).length() < 1e-9);
    assert!((s.velocity[0] - before.velocity[0]).length() < 1e-12);
    assert!((s.angular_velocity[0] - before.angular_velocity[0]).length() < 1e-15);
    assert!(s.orientation[0].angle_between(before.orientation[0]) < 1e-12);
}

/// The same flight seen from an inertial frame and from the planet's rotating frame: step
/// in one, change frames, and it must agree with stepping in the other (Coriolis and
/// centrifugal accelerations against plain gravity).
#[test]
fn rotating_and_inertial_frames_describe_the_same_motion() {
    let omega = std::f64::consts::TAU / 21_600.0;
    let start = DVec3::new(R + 20_000.0, 3_000.0, 0.0);
    let v = DVec3::new(0.0, 40.0, -(MU / start.length()).sqrt());
    let steps = 600;
    let t = DT * f64::from(steps);
    // Rotating frame B turns at ω about +Y and coincides with inertial A at t = 0.
    let to_rotating = |t: f64| {
        let (s, c) = det::sin_cos(-0.5 * omega * t);
        FrameChange {
            rotation: DQuat::from_xyzw(0.0, s, 0.0, c),
            translation: DVec3::ZERO,
            velocity: DVec3::ZERO,
            spin: DVec3::Y * -omega,
        }
    };
    let mut inertial = World::new(DT);
    inertial.planet = Some(PlanetGravity::planet(MU, DVec3::ZERO));
    inertial.add(spinning_box(start, v));
    let mut rotating = World::new(DT);
    rotating.planet = Some(PlanetGravity::planet(MU, DVec3::Y * omega));
    let mut body = spinning_box(start, v);
    body.change_frame(&to_rotating(0.0));
    rotating.add(body);
    for _ in 0..steps {
        inertial.step();
        rotating.step();
    }
    inertial.change_frame(&to_rotating(t));
    let (a, b) = (&inertial.bodies[0].structure, &rotating.bodies[0].structure);
    let error = (a.position[0] - b.position[0]).length();
    println!(
        "position {error:.2e} m, velocity {:.2e} m/s, spin {:.2e} rad/s, turn {:.2e} rad",
        (a.velocity[0] - b.velocity[0]).length(),
        (a.angular_velocity[0] - b.angular_velocity[0]).length(),
        a.orientation[0].angle_between(b.orientation[0])
    );
    // 31 km travelled at 3 km/s. With the Coriolis force from the start-of-step velocity
    // the two were 3.8 mm apart; with the end-of-step estimate 2.4 µm.
    assert!(error < 1e-5, "{error} m apart");
    assert!((a.velocity[0] - b.velocity[0]).length() < 1e-6);
    // Without the frame's fictitious torque the spin differed by Ω·|ω|·t ≈ 1.7e-3 rad/s;
    // with it 6e-6 rad/s.
    assert!((a.angular_velocity[0] - b.angular_velocity[0]).length() < 2e-5);
    assert!(a.orientation[0].angle_between(b.orientation[0]) < 2e-5);
}

#[test]
fn set_planet_reaches_every_body() {
    let mut world = World::new(DT);
    world.add(spinning_box(DVec3::X * R, DVec3::ZERO));
    let mut g = PlanetGravity::planet(0.0, DVec3::ZERO);
    g.uniform = DVec3::new(0.0, -1e-3, 0.0);
    world.set_planet(Some(g.clone()));
    assert_eq!(world.bodies[0].structure.planet, Some(g));
    assert_eq!(world.bodies[0].structure.gravity, DVec3::ZERO);
}
