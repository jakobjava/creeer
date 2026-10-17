//! The physics sandbox's scenes within their work: no tick of their first four seconds (a
//! bridge struck, a pile landing, stacks hit, a crash) may cost more Newton iterations than
//! its scene's ceiling. The work is the same on every machine, so this gate holds in CI where
//! timings cannot (`physics_bench.rs`).

use super::{PhysicsScene, run};

/// Ticks of each scene.
const TICKS: usize = 240;

#[test]
#[ignore = "measures the ceilings"]
fn measure_the_work_ceilings() {
    for (scene, name) in PhysicsScene::ALL {
        let most = run(scene, TICKS).work.into_iter().max().unwrap_or(0);
        println!("{name}: {most}");
    }
}

/// Each scene's most work in a tick (Newton iterations summed over its bodies), a tenth above
/// what it measured on 2026-10-18 (316, 207, 1355, 1520, 8, 1, 220, 2, 4, 6), brought down as
/// the solver needs fewer.
const CEILINGS: [(PhysicsScene, u64); 10] = [
    (PhysicsScene::Bridge, 350),
    (PhysicsScene::Crash, 230),
    (PhysicsScene::Stacks, 1490),
    (PhysicsScene::Rocks, 1670),
    (PhysicsScene::Cooling, 10),
    (PhysicsScene::Radiator, 2),
    (PhysicsScene::Fire, 242),
    (PhysicsScene::Station, 3),
    (PhysicsScene::AirDrop, 5),
    (PhysicsScene::Harbour, 7),
];

#[test]
fn every_scene_keeps_within_its_work() {
    for (scene, ceiling) in CEILINGS {
        let work = run(scene, TICKS).work;
        let (tick, most) = work
            .iter()
            .copied()
            .enumerate()
            .max_by_key(|&(_, w)| w)
            .unwrap_or((0, 0));
        assert!(
            most <= ceiling,
            "{scene:?}: tick {tick} took {most} Newton iterations (ceiling {ceiling})"
        );
    }
}
