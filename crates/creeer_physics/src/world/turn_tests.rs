//! The turn limit's substeps (`turn.rs`): only for boxes that touch something, and only for
//! their own group, which leaves every other group's step exactly as it would be alone.

use super::shape::{Shape, StaticShape};
use super::*;

const DT: f64 = 1.0 / 60.0;

fn crate_at(at: DVec3, half: f64) -> Body {
    let mut b = Body::rigid(
        Shape::Box {
            half: DVec3::splat(half),
        },
        40.0,
        at,
        DT,
    );
    b.friction = 0.6;
    b
}

fn ground() -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    world
}

/// Two crates stacked and one sliding at 2 m/s, all at the origin's side.
fn yard(world: &mut World) {
    world.add(crate_at(DVec3::new(0.0, 0.25, 0.0), 0.25));
    world.add(crate_at(DVec3::new(0.0, 0.75, 0.0), 0.25));
    let mut slider = crate_at(DVec3::new(2.0, 0.25, 0.0), 0.25);
    slider.structure.velocity[0] = DVec3::new(2.0, 0.0, 0.5);
    world.add(slider);
}

/// A crate spinning at 25 rad/s on the ground, far from the yard: 0.42 rad a step.
fn spinner(at: DVec3) -> Body {
    let mut b = crate_at(at, 0.25);
    b.structure.angular_velocity[0] = DVec3::new(0.0, 25.0, 0.0);
    b
}

#[test]
fn only_boxes_that_touch_something_substep_and_only_their_group() {
    let mut world = ground();
    yard(&mut world);
    let on_ground = world.add(spinner(DVec3::new(30.0, 0.25, 0.0)));
    // Flung through the air: as fast, touching nothing within the step's reach.
    let mut flying = spinner(DVec3::new(-30.0, 5.0, 0.0));
    flying.structure.angular_velocity[0] = DVec3::new(30.0, 0.0, 10.0);
    let flying = world.add(flying);
    let found = world.detect_awake();
    let plan = world.plan(&found);
    let need = |b: usize| plan.need[plan.roots[b]];
    assert_eq!(need(on_ground), 9, "0.42 rad a step at 0.05 a substep");
    assert!(
        plan.boxes.iter().all(|&(b, _)| b != flying),
        "it touches nothing"
    );
    assert_eq!(need(flying), 1);
    for b in 0..3 {
        assert_eq!(need(b), 1, "the yard's crate {b}");
    }
}
#[test]
fn a_group_stepped_beside_one_substepping_goes_as_it_would_alone() {
    let (mut alone, mut beside) = (ground(), ground());
    yard(&mut alone);
    yard(&mut beside);
    beside.add(spinner(DVec3::new(30.0, 0.25, 0.0)));
    // And a crate on the ground spun up by a torque within the step, far more than its start
    // said: the step redone for its group alone (`turn.rs`).
    let twisted = beside.add(crate_at(DVec3::new(-30.0, 0.25, 0.0), 0.25));
    let pose = |w: &World, b: usize| {
        let s = &w.bodies[b].structure;
        (s.position[0], s.orientation[0], s.velocity[0])
    };
    let (mut split, mut spun) = (0, 0.0f64);
    for tick in 0..90 {
        split += usize::from(beside.bodies[3].structure.angular_velocity[0].length() * DT > 0.05);
        let torque = if (5..8).contains(&tick) { 3000.0 } else { 0.0 };
        beside.bodies[twisted].structure.torque[0] = DVec3::new(0.0, torque, 0.0);
        alone.step();
        beside.step();
        spun = spun.max(beside.bodies[twisted].structure.angular_velocity[0].length() * DT);
        for b in 0..3 {
            assert_eq!(
                pose(&alone, b),
                pose(&beside, b),
                "crate {b} at tick {tick}"
            );
        }
    }
    // The spinner substepped for a good while (its friction spinning it down), the twisted crate
    // spun up within its steps, and the slider slid to rest.
    assert!(split > 10, "{split} ticks split");
    assert!(
        spun > 0.1,
        "the twisted crate turned {spun:.3} rad a step at most"
    );
    #[cfg(feature = "profile")]
    assert!(crate::profile::take()[crate::profile::Slot::TurnRedos as usize] > 0);
    assert!(alone.bodies[2].structure.velocity[0].length() < 1e-3);
}
