//! *bridge demo*: built, walked over, loaded and broken as scripted; what broke told in words.

use super::super::Play;
use super::super::scenes::{scene, terrain};
use super::*;

/// The scene played to `ticks`, `each` told every tick.
fn played(ticks: u32, mut each: impl FnMut(u32, &mut Play)) -> Play {
    let s = scene("bridge_demo").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    for tick in 0..ticks {
        let acts = (s.script)(tick, &play);
        play.tick(&acts);
        each(tick, &mut play);
    }
    play
}

/// The site's ids are those the scene's world gave: its items laid first.
#[test]
fn the_scenes_site_is_the_worlds() {
    let mut w = super::super::scenes::world(terrain());
    let made = w.prepare_bridge_site();
    let found = BridgeSite::of(&w, 1);
    assert_eq!(found.laid, made.laid);
    // (Its ground found again after the gully was dug: the same to rounding.)
    let off = (found.middle - made.middle).length();
    assert!(off < 1e-3, "{off} m off");
    assert_eq!((found.stand, found.across), (made.stand, made.across));
}

/// The bridge is one structure of its three stringers and twelve boards, a nail at each of their
/// 36 crossings.
#[test]
fn the_bridge_is_one_structure_nailed_at_every_crossing() {
    let s = scene("bridge_demo").expect("the scene");
    let w = (s.build)(terrain());
    let site = BridgeSite::of(&w, 1);
    let first = bridge(&site).expect("the last board");
    let parts = (0..3)
        .map(|k| site.item("stringer", k))
        .chain((0..BOARDS - 1).map(|k| site.item("deck board", k)));
    for id in parts {
        assert!(
            w.index(id.expect("laid")).is_none(),
            "{id:?} not in the bridge"
        );
    }
    let bridge = &w.world.bodies[w.index(first).expect("the bridge")].structure;
    let joints = (0..bridge.element_count())
        .filter(|&e| bridge.member(e).is_none())
        .count();
    assert_eq!(joints, 36);
}

/// In CI's length (the bridge settling on its banks, its builders setting out) nothing breaks.
/// (Its 125 elements take seconds a tick in a debug build: release builds only, ADR-0039.)
#[test]
#[cfg_attr(debug_assertions, ignore = "release builds only (ADR-0039)")]
fn the_bridge_settles_unbroken() {
    let s = scene("bridge_demo").expect("the scene");
    let play = played(s.ticks.1, |_, _| {});
    let w = &play.server.world;
    let failed = w.inspection(false).failures;
    let site = BridgeSite::of(w, 1);
    let places: Vec<(u64, f64, f64)> = (failed.iter())
        .map(|f| {
            let p = f.at - site.stand;
            (f.tick, p.dot(site.across), p.dot(site.along))
        })
        .collect();
    assert!(failed.is_empty(), "{places:?} {failed:?}");
    let first = bridge(&site).expect("the last board");
    let bridge = &w.world.bodies[w.index(first).expect("the bridge")].structure;
    assert_eq!(bridge.elements_broken(), 0);
}

/// The whole scene: it breaks under the test weights, not before, and what broke first is told
/// in words. With `BRIDGE_SAVES=<dir>` its moments are saved there for the frames.
#[test]
#[ignore = "the scene at full length: run by hand, in release"]
fn the_bridge_is_loaded_and_broken() {
    let saves = std::env::var("BRIDGE_SAVES").ok();
    let write = |name: &str, save: &creeer_sim::save::Save| {
        if let Some(dir) = &saves {
            let path = std::path::Path::new(dir).join(format!("{name}.save"));
            crate::host::write_save(&path, save).expect("written");
            println!("saved {}", path.display());
        }
    };
    let mut broke: Option<u32> = None;
    // The last seconds before the break, to see it come (a save holds no failures told).
    let mut before = std::collections::VecDeque::new();
    let s = scene("bridge_demo").expect("the scene");
    let play = played(s.ticks.0, |tick, play| {
        let failed = !play.server.world.inspection(false).failures.is_empty();
        if failed && broke.is_none() {
            broke = Some(tick);
            if let Some(save) = before.front() {
                write("before-break", save);
            }
            write("broken", &play.server.save());
        }
        if saves.is_some() && tick >= WEIGHTS && broke.is_none() && tick.is_multiple_of(30) {
            before.push_back(play.server.save());
            if before.len() > 3 {
                before.pop_front();
            }
        }
        let moments = [
            ("built", 30),
            ("walked", OVER + 150),
            ("carried", BLOCKS - 30),
            ("loaded", WEIGHTS - 10),
        ];
        if let Some((name, _)) = moments.iter().find(|(_, t)| *t == tick) {
            println!("{name} at tick {tick}");
            write(name, &play.server.save());
        }
    });
    let seen = play.server.world.inspection(false);
    for f in &seen.failures {
        println!("tick {}: {}", f.tick, f.words);
    }
    let tick = broke.expect("it never broke");
    println!("broke at tick {tick}");
    assert!(
        tick > WEIGHTS,
        "it broke at tick {tick}, before the test weights"
    );
}

/// The bridge's cost by itself and with people on it (ADR-0039's measurements): each second's
/// most and summed Newton iterations, its own, and whether it sleeps.
#[test]
#[ignore = "a measurement: run by hand, in release"]
fn what_the_bridge_costs() {
    let s = scene("bridge_demo").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    let first = bridge(&BridgeSite::of(&play.server.world, 1)).expect("it");
    let own = |play: &Play| {
        let w = &play.server.world;
        w.index(first)
            .map_or(0, |i| w.world.bodies[i].solver.iterations_total)
    };
    let (mut most, mut sum, mut mine) = (0, 0, 0);
    for tick in 0..900u32 {
        // Standing by for 5 s, then walking over and back (the script's own acts).
        let acts = if tick < 300 {
            vec![super::super::record::Act::default(); play.players()]
        } else {
            (s.script)(tick - 300 + OVER, &play)
        };
        let before = own(&play);
        let cost = play.tick(&acts);
        mine += own(&play).saturating_sub(before);
        (most, sum) = (most.max(cost.work), sum + cost.work);
        if tick % 60 == 59 {
            let w = &play.server.world;
            let asleep = (w.index(first)).is_some_and(|i| w.world.bodies[i].sleep.asleep);
            println!(
                "second {}: most {most}, all {sum}, the bridge's {mine}; asleep {asleep}",
                tick / 60
            );
            (most, sum, mine) = (0, 0, 0);
        }
    }
}
