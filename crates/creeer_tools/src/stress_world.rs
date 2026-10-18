//! The busy server stress scenario (F7.5).

use crate::lod_bench::{centres, place, walk, world, yards};
use crate::stress_scenes::{Notes, Scene, abstract_powerhouse, hash};
use creeer_data::Data;
use creeer_math::glam::DVec3;
use creeer_net::message::Command;
use creeer_server::ServerWorld;
use creeer_server::world::load_terrain;
use creeer_sim::save::SavedMachine;
use creeer_terrain::edits::{Brush, Edit, Edits};
use std::sync::{Arc, Mutex};

/// A busy server: 8 scripted players walking, dropping crates and digging among 400 bodies,
/// with 100,000 resting crates parked across the planet and 10 abstract powerhouses.
pub fn server() -> Scene {
    let (mut w, players) = world();
    for tick in 0..300 {
        walk(&mut w, &players, tick);
        w.step();
    }
    let mut save = w.to_save();
    let first = save.next_id;
    save.parked = yards(&w, 1000, 100, first);
    save.next_id = first + 100_000;
    let snapshot = abstract_powerhouse().snapshot();
    save.machines = (0..10u32)
        .map(|k| SavedMachine {
            id: k + 1,
            kind: "powerhouse".into(),
            at: place(&w, 30_000.0 + 1_000.0 * f64::from(k), 0.0, 1.0),
            born: save.tick,
            snapshot: snapshot.clone(),
        })
        .collect();
    let data = Data::base();
    let terrain = load_terrain(&data, "planets/home.ron").expect("home.ron");
    let w = Arc::new(Mutex::new(ServerWorld::from_save(data, save, terrain)));
    let (state, seen) = (Arc::clone(&w), Arc::clone(&w));
    Scene {
        tick: Box::new(move |tick| {
            let mut w = w.lock().expect("world");
            let t = u64::from(tick);
            let scope = creeer_core::profile::scope("players walk");
            walk(&mut w, &players, t);
            drop(scope);
            let c = centres(&w, &players);
            // Every two seconds one player drops a crate; every ten one digs.
            let who = (tick / 120) as usize % players.len();
            if tick % 120 == 0 {
                let at = c[who] + c[who].normalize() * 3.0 + DVec3::X * 2.0;
                let _ = w.apply(&Command::Crate(at), Some(players[who]));
            }
            if tick % 600 == 300 {
                let mut edits = Edits::default();
                let center = w.ground_under(c[who] + DVec3::Z * 6.0);
                edits.push(Edit::Dig(Brush::Sphere {
                    center,
                    radius: 1.5,
                }));
                let _ = w.apply(&Command::Edit(edits.to_bytes()), Some(players[who]));
            }
            w.regions(&c);
            w.step();
        }),
        hash: Box::new(move || {
            let mut w = state.lock().expect("world");
            w.join_machines();
            hash(w.to_save().to_bytes())
        }),
        notes: Box::new(move || {
            let w = seen.lock().expect("world");
            let (islands, bodies) = w.parked();
            Notes {
                text: format!(
                    "{bodies} parked in {islands} islands, {} machines",
                    w.machines().len()
                ),
                gauges: Vec::new(),
            }
        }),
        paced: false,
    }
}
