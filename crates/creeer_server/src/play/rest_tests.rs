//! G2.0's rest (ADR-0024, "G2.0: rest"): felled trunks lying on the terrain come to rest.

use super::Play;
use super::record::Act;
use super::scenes::{scene, terrain};
use creeer_physics::world::World;
use creeer_sim::TerrainGround;
use std::sync::Arc;

/// *lumberjack*'s felled parts 10 s after the scene (its players taken out), as the save
/// format's version 12 wrote them before G2.0: four trunks of 69–125 kg, three of them still
/// rocking and creeping then (`rest_probe.rs`).
const FELLED: &[u8] = include_bytes!("fixtures/lumberjack-felled.bin");

/// The felled parts on the scene's terrain.
fn felled() -> World {
    let mut world = World::from_bytes_of(FELLED, 12).expect("the felled parts");
    world.ground = Some(Arc::new(TerrainGround(terrain())));
    world
}

/// The trunk that crept downhill alone (82 kg, 19 m): before G2.0 a bump under it was found,
/// lost and found again, each time holding tens of millijoules, and it slid in jerks of a
/// millimetre (10 mm in 1.5 s) and never slept. With its bumps kept it sleeps within 5 s
/// (3.7 s measured) and stays asleep.
#[test]
fn a_felled_trunk_alone_on_the_terrain_comes_to_rest() {
    let mut world = felled();
    world.retain_bodies(|i, _| i == 1);
    let mut slept = None;
    for tick in 0..360u32 {
        world.step();
        if world.bodies[0].sleep.asleep {
            slept.get_or_insert(tick);
        } else {
            slept = None;
        }
    }
    let s = &world.bodies[0].structure;
    assert!(
        slept.is_some_and(|t| t < 300),
        "asleep from tick {slept:?}: {} m/s, {} rad/s",
        s.velocity[0].length(),
        s.angular_velocity[0].length()
    );
}

/// The gate (G2.0): *lumberjack* played, then left alone, every felled part asleep within 5 s
/// of the last blow. Measured by hand: 40 s of play. Not met yet (ADR-0024, "G2.0: rest").
#[test]
#[ignore = "measured by hand: the whole scene"]
fn lumberjack_s_trunks_sleep_within_5_s_of_the_last_blow() {
    let s = scene("lumberjack").expect("the scene");
    let terrain = terrain();
    let mut play = Play::new((s.build)(terrain.clone()), s.name, s.players, &terrain);
    let still = vec![Act::default(); s.players.len()];
    let mut last_blow = 0;
    for tick in 0..s.ticks.0 {
        let acts = (s.script)(tick, &play);
        play.tick(&acts);
        if !play.server.world.take_blows().is_empty() {
            last_blow = tick;
        }
    }
    let mut tick = s.ticks.0;
    let mut since = None;
    // Each part's last tick awake, and the ticks' cost from 10 s after the last blow.
    let mut last_awake = vec![0; play.server.world.world.bodies.len()];
    let (mut ms, mut lying) = (0.0, 0);
    while tick < last_blow + 900 {
        let t = play.tick(&still);
        if tick >= last_blow + 600 {
            (ms, lying) = (ms + t.ms, lying + 1);
        }
        let w = &play.server.world.world;
        for (i, b) in w.bodies.iter().enumerate() {
            if !b.held && !b.sleep.asleep && i < last_awake.len() {
                last_awake[i] = tick;
            }
        }
        let all = w.bodies.iter().all(|b| b.held || b.sleep.asleep);
        if all {
            since.get_or_insert(tick);
        } else {
            since = None;
        }
        tick += 1;
    }
    let w = &play.server.world.world;
    let awake: Vec<String> = (w.bodies.iter().enumerate())
        .filter(|(_, b)| !b.held && !b.sleep.asleep)
        .map(|(i, b)| {
            let s = &b.structure;
            format!(
                "{i} ({:.0} kg): {:.1} mm/s, {:.1} mrad/s",
                s.total_mass(),
                s.velocity[0].length() * 1e3,
                s.angular_velocity[0].length() * 1e3
            )
        })
        .collect();
    let after = since.map(|t| f64::from(t.saturating_sub(last_blow)) / 60.0);
    println!("last blow at tick {last_blow}; all asleep {after:?} s after it; awake: {awake:?}");
    let slept: Vec<String> = (last_awake.iter().enumerate())
        .filter(|(_, t)| **t > last_blow)
        .map(|(i, t)| format!("{i}: {:.1} s", f64::from(t - last_blow) / 60.0))
        .collect();
    println!(
        "last awake after the blow: {slept:?}; {:.2} ms a tick from 10 to 15 s after it",
        ms / f64::from(lying.max(1))
    );
    assert!(after.is_some_and(|t| t <= 5.0), "awake: {awake:?}");
}
