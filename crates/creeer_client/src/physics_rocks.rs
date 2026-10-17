//! The physics sandbox's rock pile: convex hulls (`hulls.rs`) dropped onto a ramp, with a
//! hexagonal prism lying across it and wedges at its foot.

use crate::hulls::{PRISM, ROCKS, WEDGE, catalogue};
use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3};
use creeer_physics::world::shape::{Shape, StaticShape};
use creeer_physics::world::surface::Surface;
use creeer_physics::world::{Body, World};

/// Density of the rocks (granite, kg/m³).
const GRANITE: f64 = 2700.0;
/// The ramp's slope (degrees).
const SLOPE: f64 = 35.0;
/// Half the ramp's length, thickness and width (m).
const RAMP: DVec3 = DVec3::new(3.0, 0.2, 2.0);

/// Builds the rock pile in `world`.
pub fn rocks(world: &mut World) {
    let dt = world.dt();
    world.static_friction = 0.6;
    // The ramp, tilted down towards +x, its top face's lower end on the ground at x = 3 m.
    let (sin, cos) = det::sin_cos(SLOPE.to_radians());
    let (up_slope, normal) = (DVec3::new(-cos, sin, 0.0), DVec3::new(sin, cos, 0.0));
    let foot = DVec3::new(3.0, 0.0, 0.0);
    world.statics.push(StaticShape::Box {
        center: foot + up_slope * RAMP.x - normal * RAMP.y,
        rotation: DQuat::from_rotation_z(-SLOPE.to_radians()),
        half: RAMP,
    });
    let on_ramp = |along: f64, z: f64| foot + up_slope * along + DVec3::Z * z;
    let hulls = catalogue();
    let rock = |k: usize, at: DVec3, turn: DQuat| {
        let hull = hulls[k].clone();
        let mut body = Body::rigid(Shape::Hull(hull.clone()), GRANITE * hull.volume(), at, dt);
        body.structure.orientation[0] = turn;
        body.surface = Surface::ROCK;
        body.friction = 0.6;
        body
    };
    // Eighteen rocks above the ramp's upper half, three rows deep.
    for row in 0..3u32 {
        for col in 0..6u32 {
            let k = (row * 6 + col) as usize;
            let at = on_ramp(3.0 + 0.5 * f64::from(col % 3), -1.2 + 0.48 * f64::from(col))
                + DVec3::Y * (1.0 + 0.8 * f64::from(row));
            let axis = DVec3::new(f64::from(col) + 1.0, f64::from(row) + 0.5, 1.0);
            let turn = DQuat::from_axis_angle(axis.normalize(), 0.7 * f64::from(row * 6 + col + 1));
            world.add(rock(k % ROCKS, at, turn));
        }
    }
    // A hexagonal prism lying across the ramp on one of its faces, near the top.
    let lying = 0.3 * 0.75f64.sqrt();
    let prism = on_ramp(5.0, 0.0) + normal * lying;
    world.add(rock(
        PRISM,
        prism,
        DQuat::from_rotation_z(-SLOPE.to_radians()),
    ));
    // Three wedges at the foot, their thin ends towards it.
    let centroid = 0.5 / 3.0;
    for z in [-1.2, 0.0, 1.2] {
        let at = DVec3::new(4.2, centroid, z);
        world.add(rock(
            WEDGE,
            at,
            DQuat::from_rotation_y(std::f64::consts::PI),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pile's first four seconds, its busiest (the rocks landing and sliding down the ramp
    /// into each other), take a bounded number of Newton iterations: about 80,000 summed over
    /// the bodies, where the contacts' stiffness frozen at each step's start took 1.1 million
    /// (30–90 ms a step, F6).
    #[test]
    fn the_rock_pile_converges_in_few_iterations() {
        let mut world = World::new(1.0 / 60.0);
        world.statics.push(StaticShape::Plane {
            normal: DVec3::Y,
            offset: 0.0,
        });
        rocks(&mut world);
        let mut iterations = 0;
        for _ in 0..240 {
            world.step();
            iterations += world
                .bodies
                .iter()
                .map(|b| u64::from(b.solver.last_iterations))
                .sum::<u64>();
        }
        assert!(iterations < 120_000, "{iterations} iterations");
    }
}
