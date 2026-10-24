//! Static friction between bodies, kept from step to step (G2.0, ADR-0024): a face's patch
//! has its corners on body A, whose friction points and anchors are both material points.

use super::shape::{Shape, StaticShape};
use super::*;
use creeer_math::det;

const DT: f64 = 1.0 / 60.0;

/// A 40 kg crate on a 400 kg crate on a 20° ramp (μ = 0.6 everywhere, tan 20° = 0.36): how far
/// the top crate moves across the bottom one in 2 s once set down (m). `top_first`: the top
/// crate is added first, so it is body A of their pair as often as B.
fn creep(top_first: bool) -> f64 {
    let angle = 20f64.to_radians();
    let (sin, cos) = det::sin_cos(angle);
    let normal = DVec3::new(-sin, cos, 0.0);
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal,
        offset: 0.0,
    });
    world.static_friction = 0.6;
    let crate_at = |half: DVec3, mass: f64, height: f64| {
        let mut body = Body::rigid(Shape::Box { half }, mass, normal * height, DT);
        body.structure.orientation[0] = rotation(DVec3::Z, angle);
        body.friction = 0.6;
        body
    };
    let bottom = crate_at(DVec3::new(0.6, 0.3, 0.6), 400.0, 0.3);
    let top = crate_at(DVec3::splat(0.2), 40.0, 0.8);
    let (b, t) = if top_first {
        let t = world.add(top);
        (world.add(bottom), t)
    } else {
        let b = world.add(bottom);
        (b, world.add(top))
    };
    let across = |w: &World| {
        let (sb, st) = (&w.bodies[b].structure, &w.bodies[t].structure);
        sb.orientation[0].conjugate() * (st.position[0] - sb.position[0])
    };
    for _ in 0..30 {
        world.step();
    }
    let start = across(&world);
    for _ in 0..120 {
        world.step();
    }
    (across(&world) - start).length()
}

/// The top crate stays put on the bottom one, whichever is body A. Before G2.0 a patch with
/// its corners on B forgot each step's slip (A's point fell under B's corner again), and the
/// crate crept.
#[test]
fn a_crate_on_a_crate_on_a_ramp_sticks() {
    for top_first in [false, true] {
        let moved = creep(top_first);
        assert!(
            moved < 1e-5,
            "top first {top_first}: crept {moved} m in 2 s"
        );
    }
}
