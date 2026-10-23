//! An impulse moves a body's point as its effective mass says.

use super::*;
use crate::world::Body;
use crate::world::shape::Shape;

const DT: f64 = 1.0 / 60.0;

#[test]
fn a_struck_point_moves_by_the_impulse_over_its_effective_mass() {
    let mut world = World::new(DT);
    let half = DVec3::new(0.5, 0.3, 0.2);
    let i = world.add(Body::rigid(Shape::Box { half }, 50.0, DVec3::ZERO, DT));
    // At its centre: its whole mass.
    assert!((world.effective_mass(i, 0, DVec3::ZERO, DVec3::X) - 50.0).abs() < 1e-9);
    // 0.3 m up, along x: turned too (I_zz = m(4h_x² + 4h_y²)/12).
    let point = DVec3::Y * 0.3;
    let izz = 50.0 * (4.0 * 0.25 + 4.0 * 0.09) / 12.0;
    let want = 1.0 / (1.0 / 50.0 + 0.09 / izz);
    let m = world.effective_mass(i, 0, point, DVec3::X);
    assert!((m - want).abs() < 1e-9, "{m} against {want}");
    // Struck there: the point's speed along x is the impulse over that mass.
    world.apply_impulse(i, 0, DVec3::X * 10.0, point);
    let s = &world.bodies[i].structure;
    let v = s.velocity[0] + s.angular_velocity[0].cross(point - s.position[0]);
    assert!(
        (v.x - 10.0 / m).abs() < 1e-9,
        "{} against {}",
        v.x,
        10.0 / m
    );
}
