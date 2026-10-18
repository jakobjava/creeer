//! Machines in the world: active near a player, abstract far from all, caught up and back
//! when one comes again, kept by saves.

use super::machines::CATCH;
use super::tests::world;
use super::*;
use creeer_sim::abstraction::{Machine, Mode};
use creeer_sim::lod;
use creeer_sim::powerhouse::Powerhouse;
use creeer_sim::save::Save;

/// Steps `w` for `seconds` with its LOD pass around `centre`.
fn run(w: &mut ServerWorld, seconds: f64, centre: DVec3) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for _ in 0..(seconds / DT).round() as u64 {
        w.regions(&[centre]);
        w.step();
    }
}

fn mode(w: &ServerWorld) -> Option<Mode> {
    w.machines()[0].mode()
}

#[test]
fn a_machine_goes_abstract_far_away_and_comes_back_where_it_would_be() {
    let mut w = world();
    let at = w.spawn();
    w.apply(&Command::Powerhouse(at), None).unwrap();
    let away = at + at.normalize().cross(DVec3::X).normalize() * (lod::PARK + 500.0);
    run(&mut w, 30.0, at);
    assert_eq!(mode(&w), Some(Mode::Active));
    // Everyone far: abstract once steady (from rest: about 200 s; from its own last windows,
    // no probe).
    run(&mut w, 300.0, away);
    assert_eq!(mode(&w), Some(Mode::Abstract));
    let house = w.machines()[0].house().unwrap();
    assert_eq!(house.probes, 0);
    run(&mut w, 300.0, away);
    // A player comes near: caught up beside the ticks, then back.
    run(&mut w, 0.5, at);
    assert_eq!(mode(&w), None, "catching up");
    #[allow(clippy::cast_precision_loss)]
    run(&mut w, CATCH as f64 * DT + 0.5, at);
    assert_eq!(mode(&w), Some(Mode::Active));
    // Where a powerhouse run in full as long is.
    let born = w.machines()[0].born;
    let mut full = Powerhouse::new();
    for _ in born..w.tick {
        full.tick();
    }
    let house = w.machines()[0].house().unwrap();
    let (got, want) = (house.amounts(), full.amounts());
    #[allow(clippy::cast_precision_loss)]
    let world_time = (w.tick - born) as f64 * DT;
    assert!(
        (house.time() - world_time).abs() < 1e-6,
        "{} against {world_time}",
        house.time()
    );
    for k in 0..got.len() {
        let error = (got[k] - want[k]).abs() / want[k].abs().max(1e-9);
        assert!(error < 0.01, "stock {k}: {} against {}", got[k], want[k]);
    }
}

#[test]
fn an_abstract_machine_is_saved_and_loaded() {
    let mut w = world();
    let at = w.spawn();
    w.apply(&Command::Powerhouse(at), None).unwrap();
    let away = at * 1.01;
    run(&mut w, 30.0, at);
    run(&mut w, 300.0, away);
    assert_eq!(mode(&w), Some(Mode::Abstract));
    w.join_machines();
    let bytes = w.to_save().to_bytes();
    let loaded = ServerWorld::from_save(
        creeer_data::Data::base(),
        Save::from_bytes(&bytes).unwrap(),
        super::tests::terrain(),
    );
    assert_eq!(loaded.machines().len(), 1);
    assert_eq!(loaded.machines()[0].mode(), Some(Mode::Abstract));
    assert_eq!(loaded.machine_views(), w.machine_views());
    assert_eq!(loaded.to_save().to_bytes(), bytes);
}
