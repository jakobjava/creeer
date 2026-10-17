//! The avatar rolls at the speed asked for, stops, jumps about a metre (and only from the
//! ground), climbs a walkable slope, and rests asleep when left alone.

use super::*;
use creeer_physics::world::shape::StaticShape;

const DT: f64 = 1.0 / 60.0;

/// A world with level ground (μ = 0.9) and an avatar standing on it at the origin.
fn on_ground(normal: DVec3) -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal,
        offset: 0.0,
    });
    world.static_friction = 0.9;
    world.add(body(normal * RADIUS, DT));
    for _ in 0..30 {
        steer(&mut world, 0, (DVec3::ZERO, false), DVec3::Y);
        world.step();
    }
    world
}

fn run(world: &mut World, walk: DVec3, jump: bool, steps: usize) {
    for _ in 0..steps {
        steer(world, 0, (walk, jump), DVec3::Y);
        world.step();
    }
}

fn velocity(world: &World) -> DVec3 {
    world.bodies[0].structure.velocity[0]
}

#[test]
fn it_rolls_at_the_speed_asked_for_and_stops() {
    let mut world = on_ground(DVec3::Y);
    let walk = DVec3::new(3.0, 0.0, 4.0); // 5 m/s
    // Limited by its grip: 5 m/s takes 0.56 s at 9 m/s².
    run(&mut world, walk, false, 30);
    let early = velocity(&world);
    assert!(
        early.length() > 2.5 && early.length() < 4.8,
        "{early} after 0.5 s"
    );
    run(&mut world, walk, false, 60);
    let v = velocity(&world);
    assert!((v - walk).length() < 0.05, "{v} for {walk}");
    assert!(v.y.abs() < 1e-3, "it hops: {}", v.y);
    // Rolling, not sliding: ω × r = v at the contact.
    let w = world.bodies[0].structure.angular_velocity[0];
    let slip = v + w.cross(DVec3::NEG_Y * RADIUS);
    assert!(slip.length() < 0.05, "slips at {slip}");
    // Let go of the keys: it stops within a second, and soon sleeps.
    run(&mut world, DVec3::ZERO, false, 60);
    assert!(
        velocity(&world).length() < 0.05,
        "{} after 1 s",
        velocity(&world)
    );
    run(&mut world, DVec3::ZERO, false, 240);
    assert!(
        world.bodies[0].sleep.asleep,
        "awake at {}",
        velocity(&world)
    );
}

#[test]
fn it_jumps_about_a_metre_and_only_from_the_ground() {
    let mut world = on_ground(DVec3::Y);
    let start = world.bodies[0].structure.position[0].y;
    let mut top = start;
    let mut jumps = 0;
    for step in 0..120 {
        // Jump held all along: one jump, not a climb into the sky.
        let grounded = steer(&mut world, 0, (DVec3::ZERO, true), DVec3::Y);
        if grounded && step > 0 && velocity(&world).y < 0.1 {
            jumps += usize::from(top - start > 0.5);
        }
        world.step();
        top = top.max(world.bodies[0].structure.position[0].y);
        if step == 60 {
            break;
        }
    }
    // v²/2g = 1.03 m.
    let height = top - start;
    let expected = JUMP_SPEED * JUMP_SPEED / (2.0 * 9.806_65);
    assert!(
        (height - expected).abs() < 0.08 * expected,
        "{height} m for {expected}"
    );
    assert!(jumps <= 1, "{jumps} jumps");
}

#[test]
fn it_climbs_a_walkable_slope_and_not_a_cliff() {
    for (degrees, climbs) in [(20.0f64, true), (60.0, false)] {
        let (sin, cos) = creeer_math::det::sin_cos(degrees.to_radians());
        let normal = DVec3::new(-sin, cos, 0.0);
        let uphill = DVec3::new(cos, sin, 0.0);
        let mut world = on_ground(normal);
        let start = world.bodies[0].structure.position[0];
        run(&mut world, uphill * 3.0, false, 180);
        let climbed = (world.bodies[0].structure.position[0] - start).dot(uphill);
        if climbs {
            assert!(climbed > 4.0, "{climbed} m up {degrees}°");
        } else {
            assert!(climbed < 0.0, "{climbed} m up {degrees}°");
        }
    }
}
