//! The bridge site (ADR-0037): its gully as wide and deep as it is meant, its parts and loads
//! a few steps from the start and at rest (a site at rest costs no ticks), and all of it saved.

use super::*;
use crate::play::scenes::{terrain, world};

/// The site made on the reference planet's spawn.
fn site() -> (ServerWorld, BridgeSite) {
    let mut w = world(terrain());
    let site = w.prepare_bridge_site();
    (w, site)
}

#[test]
fn the_gully_is_as_wide_and_deep_as_meant_and_a_stringer_spans_it() {
    let (w, site) = site();
    let up = site.middle.normalize();
    // Solid ground under a point `x` m across from the middle, how far below the start's level.
    let depth = |x: f64| {
        let top = site.middle + site.across * x + up * 0.5;
        (top - w.ground_under(top)).dot(up) - 0.5
    };
    assert!((depth(0.0) - DEPTH).abs() < 0.1, "{:.2} m deep", depth(0.0));
    for side in [-1.0, 1.0] {
        // Inside its rims it falls away; beyond them the bank is as it was.
        assert!(
            depth(side * 0.9) > 0.5,
            "{:.2} at {side}",
            depth(side * 0.9)
        );
        assert!(
            depth(side * 1.4).abs() < 0.08,
            "{:.2} at {side}",
            depth(side * 1.4)
        );
    }
    // A 3 m stringer rests 0.3 m or more on each bank.
    let stringer = w.kit_named("stringer").and_then(|i| i.extents()).unwrap();
    assert!(stringer[0] >= WIDTH + 0.6, "{stringer:?}");
    // Along it, as deep 4 m north and south of the middle.
    for y in [-4.0, 4.0] {
        let top = site.middle + site.along * y + up * 0.5;
        let d = (top - w.ground_under(top)).dot(up) - 0.5;
        assert!((d - DEPTH).abs() < 0.15, "{d:.2} m deep at {y}");
    }
}

/// Every part and load lies within a few steps of the start on the near bank, and comes to rest
/// within three seconds; from then on a tick of the site costs no Newton iterations.
#[test]
fn the_parts_lie_a_few_steps_away_and_come_to_rest() {
    let (mut w, site) = site();
    assert_eq!(site.laid.len(), 50, "{:?}", site.laid);
    let near_rim = OUT - WIDTH * 0.5;
    for (name, id) in &site.laid {
        let i = w.index(*id).unwrap();
        let p = w.world.bodies[i].structure.position[0] - site.stand;
        let (x, y) = (p.dot(site.across), p.dot(site.along));
        assert!(
            creeer_math::det::hypot(x, y) < 4.5,
            "{name} {x:.1} {y:.1} m away"
        );
        assert!(x < near_rim - 0.5, "{name} {x:.1} m across, by the gully");
    }
    let iterations = |w: &ServerWorld| -> u64 {
        (w.world.bodies.iter())
            .map(|b| b.solver.iterations_total)
            .sum()
    };
    for _ in 0..180 {
        w.step();
    }
    let awake: Vec<&String> = (site.laid.iter())
        .filter(|(_, id)| {
            w.index(*id)
                .is_some_and(|i| !w.world.bodies[i].sleep.asleep)
        })
        .map(|(n, _)| n)
        .collect();
    assert!(awake.is_empty(), "still awake after 3 s: {awake:?}");
    let before = iterations(&w);
    for _ in 0..60 {
        w.step();
    }
    assert_eq!(iterations(&w), before, "a second of the site at rest");
    // Nothing moved off where it was laid by more than it settles.
    for (name, id) in &site.laid {
        let i = w.index(*id).unwrap();
        let p = w.world.bodies[i].structure.position[0] - site.stand;
        assert!(
            p.dot(site.across) < near_rim - 0.4,
            "{name} rolled towards the gully"
        );
    }
}

/// The site is the world's like anything else: its gully is in the save's edits, its parts are
/// bodies of the save.
#[test]
fn the_site_is_saved_as_the_world() {
    let (w, site) = site();
    let save = w.to_save();
    let back = ServerWorld::from_save(w.data.clone(), save, terrain());
    let up = site.middle.normalize();
    let top = site.middle + up * 0.5;
    let depth = (top - back.ground_under(top)).dot(up) - 0.5;
    assert!(
        (depth - DEPTH).abs() < 0.1,
        "{depth:.2} m deep after loading"
    );
    for (name, id) in &site.laid {
        assert!(back.index(*id).is_some(), "{name} not in the save");
    }
}
