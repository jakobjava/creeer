//! Ages in the world: a dropped crate is worn by its landing and weathers in the air; the sea
//! weathers three times as fast; ages survive a save.

use super::tests::{above, terrain, world};
use super::*;
use creeer_sim::ageing::{AIR, PACE, SEA};
use creeer_sim::save::Save;

/// Drops a crate from `height` m and lets it land and settle; its id.
fn dropped_crate(w: &mut ServerWorld, height: f64) -> EntityId {
    w.apply(&Command::Crate(above(w, 3.0, height)), None)
        .unwrap();
    let id = *w.ids().last().expect("the crate");
    for _ in 0..180 {
        w.step();
    }
    id
}

#[test]
fn a_dropped_crate_is_worn_by_its_landing_and_weathers_in_the_air() {
    let mut w = world();
    let id = dropped_crate(&mut w, 3.0);
    let age = w.age(id).expect("the crate has an age");
    // Its bottom fell about 2.5 m: some 4.9 kJ for 200 kg, a tenth of FULL_WEAR. Bounces
    // dissipate a little more; nothing near battered.
    assert!(age.wear > 0.05 && age.wear < 0.2, "wear {}", age.wear);
    // Outdoors in air, at the world's pace.
    assert!((age.rate - PACE * AIR).abs() < 1e-18);
    let later = age.exposure_at(w.tick + 6 * 3600 * 60, DT);
    assert!(
        (later - age.exposure_at(w.tick, DT) - 1.0).abs() < 1e-9,
        "a year a game day"
    );
    // One placed gently on the ground is not worn.
    let mut calm = world();
    let gentle = dropped_crate(&mut calm, 0.52);
    let calm_wear = calm.age(gentle).expect("an age").wear;
    assert!(calm_wear < 1e-3);
}

#[test]
fn the_sea_weathers_three_times_as_fast_as_the_air() {
    let w = world();
    let up = w.spawn().normalize();
    let ground = w.spawn();
    let sea = w
        .surroundings
        .sea
        .as_ref()
        .expect("the home planet has a sea");
    // Well below the sea's surface along the spawn's vertical, and well above it.
    let deep = sea.surface(ground).0 - up * 50.0;
    let high = sea.surface(ground).0 + up * 50.0;
    assert!((w.weathering_rate(deep) - PACE * SEA).abs() < 1e-18);
    assert!((w.weathering_rate(high) - PACE * AIR).abs() < 1e-18);
}

#[test]
fn ages_survive_a_save_and_go_on_after_it() {
    let mut w = world();
    let id = dropped_crate(&mut w, 2.0);
    let before = w.age(id).expect("an age");
    let bytes = w.to_save().to_bytes();
    let mut loaded = ServerWorld::from_save(
        creeer_data::Data::base(),
        Save::from_bytes(&bytes).unwrap(),
        terrain(),
    );
    assert_eq!(loaded.age(id), Some(before));
    for _ in 0..60 {
        loaded.step();
    }
    let after = loaded.age(id).expect("an age");
    assert!(after.exposure_at(loaded.tick, DT) > before.exposure_at(w.tick, DT));
    assert_eq!(after.wear, before.wear, "resting: no more wear");
}
