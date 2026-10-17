//! Curved ground against analytic results: its curvature, and the normal force of a ball
//! rolling over bumps.

use super::planet_tests::FieldGround;
use super::shape::Shape;
use super::*;
use creeer_math::det;
use creeer_math::glam::DMat3;

const DT: f64 = 1.0 / 60.0;

#[test]
fn the_ground_s_curvature_is_its_shape_operator() {
    // A sphere of radius 10 m curves away by t²/20 in every direction: S = −I/10.
    let sphere = FieldGround(|p: DVec3| p.length() - 10.0);
    let s = super::ground::curvature(&sphere, DVec3::Y * 10.0, DVec3::Y).expect("curvature");
    let want = DMat3::from_diagonal(DVec3::new(-0.1, 0.0, -0.1));
    assert!(
        (s - want).abs().to_cols_array().iter().all(|&d| d < 1e-6),
        "{s}"
    );
    // The bottom of a sine valley of amplitude A and length λ: κ = A/λ² along it, 0 across.
    let valley = FieldGround(|p: DVec3| p.y - 0.005 * det::sin(p.x / 0.3));
    let bottom = DVec3::new(-0.3 * std::f64::consts::FRAC_PI_2, -0.005, 0.0);
    let s = super::ground::curvature(&valley, bottom, DVec3::Y).expect("curvature");
    assert!(
        (s.x_axis.x - 0.005 / 0.09).abs() < 1e-4 && s.z_axis.z.abs() < 1e-9,
        "{s}"
    );
}

/// A ball rolling down a 15° slope over bumps (5 mm high, 1.9 m long) at up to 3 m/s, in 60 Hz
/// steps: the ground under it is measured afresh at every evaluation and its dashpot damps
/// only its own vibration, so its normal force (the elastic force friction uses) is the
/// analytic m·(g·n + v²·κ), κ the curvature of its centre's path, within 3 % of its weight
/// at every tick and 0.5 % on average, and it gains no energy. Against ground frozen for the
/// step and a dashpot seven times overdamped, the force swung from 108 to 1,125 N; with the
/// dashpot on the bodies' velocity instead of the gap's rate it was 6 % off at worst and
/// 1.7 % on average (F5).
#[test]
fn a_ball_rolls_over_bumps_with_its_true_normal_force() {
    let (sin, cos) = det::sin_cos(15f64.to_radians());
    let normal = DVec3::new(-sin, cos, 0.0);
    let bumps = move |p: DVec3| p.dot(normal) - 0.005 * det::sin(p.x / 0.3);
    let mut world = World::new(DT);
    world.ground = Some(Arc::new(FieldGround(bumps)));
    world.static_friction = 0.8;
    let (radius, mass) = (0.3, 30.0);
    let mut b = Body::rigid(Shape::Sphere { radius }, mass, normal * radius, DT);
    b.friction = 0.8;
    world.add(b);
    let weight = mass * 9.806_65 * cos;
    // The force that keeps the centre on its path: gravity's normal part and the centripetal
    // force of its speed along it. The centre's path curves as κ/(1 − r·κ).
    let analytic = |world: &World| {
        let s = &world.bodies[0].structure;
        let (c, v) = (s.position[0], s.velocity[0]);
        let ground = FieldGround(bumps);
        let (q, n) = super::ground::closest(&ground, c, c - normal * radius).expect("ground");
        let shape = super::ground::curvature(&ground, q, n).expect("curvature");
        let along = v - n * n.dot(v);
        let t = along.normalize_or_zero();
        let kappa = t.dot(shape * t);
        mass * (9.806_65 * n.y + along.length_squared() * kappa / (1.0 - radius * kappa))
    };
    world.step();
    let mut previous = world.energy();
    let mut total_error = 0.0;
    for tick in 0..100 {
        world.step();
        let force = world.contacts.first().map_or(0.0, |c| c.normal_force());
        if tick > 10 {
            let error = (force - analytic(&world)).abs() / weight;
            assert!(
                error < 0.03,
                "tick {tick}: {force} N, {error} of the weight off"
            );
            total_error += error;
        }
        let energy = world.energy();
        let moving = world.bodies[0].structure.node_energy().0;
        assert!(
            energy <= previous + 1e-3 * moving,
            "tick {tick}: {previous} -> {energy} J"
        );
        previous = energy;
    }
    let mean = total_error / 89.0;
    assert!(mean < 0.005, "{mean} of the weight off on average");
    assert!(
        world.bodies[0].structure.velocity[0].length() > 3.0,
        "it rolled"
    );
}
