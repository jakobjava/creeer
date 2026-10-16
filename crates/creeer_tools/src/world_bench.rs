//! F2 contact world: cost of one tick for many touching rigid bodies.

use crate::spikes::{Better, DT, Report, p, time_ticks};
use creeer_math::glam::DVec3;
use creeer_physics::world::shape::{Shape, StaticShape};
use creeer_physics::world::{Body, World};

/// Ten stacks of ten 1 m boxes (100 bodies, 400+ contacts) on the ground.
fn stacks() -> World {
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    for stack in 0..10 {
        for level in 0..10 {
            let position = DVec3::new(f64::from(stack) * 1.5, 0.5 + f64::from(level), 0.0);
            world.add(Body::rigid(
                Shape::Box {
                    half: DVec3::splat(0.5),
                },
                100.0,
                position,
                DT,
            ));
        }
    }
    world
}

pub fn run(report: &mut Report) {
    println!("F2 contact world: 10 stacks of 10 boxes (ms per tick, p50 / p99)");
    // The best of three fresh worlds: a single 60-tick run varies by ±12 % between identical
    // runs (scheduling), too much for the 10 % regression check.
    let (mut world, mut awake) = (stacks(), None::<creeer_core::TimingStats>);
    for _ in 0..3 {
        let mut fresh = stacks();
        let run = time_ticks(|| fresh.step(), 60);
        if awake
            .as_ref()
            .is_none_or(|best| p(&run, 50.0) < p(best, 50.0))
        {
            (world, awake) = (fresh, Some(run));
        }
    }
    let awake = awake.expect("three runs");
    println!(
        "  settling, all awake          | {:.3} / {:.3}  ({} contacts)",
        p(&awake, 50.0),
        p(&awake, 99.0),
        world.contacts.len()
    );
    report.metric(
        "world_100_boxes_awake_p50_ms",
        p(&awake, 50.0),
        Better::Lower,
    );
    for _ in 0..120 {
        world.step();
    }
    let asleep = time_ticks(|| world.step(), 60);
    println!(
        "  settled, {:>3} asleep          | {:.3} / {:.3}",
        world.sleeping(),
        p(&asleep, 50.0),
        p(&asleep, 99.0)
    );
    report.metric(
        "world_100_boxes_asleep_p50_ms",
        p(&asleep, 50.0),
        Better::Info,
    );
    let (mut drift, mut speed) = (0.0f64, 0.0f64);
    for (k, b) in world.bodies.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x0 = (k / 10) as f64 * 1.5;
        let p = b.structure.position[0];
        drift = drift.max((DVec3::new(p.x - x0, 0.0, p.z)).length());
        speed = speed.max(b.structure.velocity[0].length());
    }
    println!("  after 4 s: largest sideways drift {drift:.2e} m, largest speed {speed:.2e} m/s");
    println!();
}
