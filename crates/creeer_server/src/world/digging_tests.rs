//! Digging in the world (G1.5, ADR-0032): a pick's blow on the ground lays a heap of what it
//! broke out, of the ground's density and composition; enough dug at a spot cuts a pit; mass is
//! kept over 10,000 blows; bare hands dig soil but not rock; a stone head wears.

use super::super::tests::world;
use super::*;
use creeer_net::message::Target;
use creeer_sim::items::{Form, Piece};
use creeer_terrain::geology::deposits::tangent_frame;
use std::time::Instant;

fn wait(w: &mut ServerWorld, ada: EntityId, n: u32) {
    for _ in 0..n {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
}

/// Ada standing, holding the pickaxe (or nothing), and a point on the ground 1.2 m east of her.
fn ada(pick: bool) -> (ServerWorld, EntityId, DVec3) {
    let mut w = world();
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60);
    if pick {
        w.apply(&Command::Wield { tool: Some(0) }, Some(ada))
            .unwrap();
    }
    let at = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let (east, _) = tangent_frame(character::plumb(&w.world, at));
    let spot = w.ground_under(at + east * 1.2);
    (w, ada, spot)
}

/// Ada strikes the ground at `at`, and the blow falls.
fn strike(w: &mut ServerWorld, ada: EntityId, at: DVec3) {
    #[allow(clippy::cast_precision_loss)]
    let now = w.tick as f64;
    let target = Target {
        body: None,
        at,
        seen: now,
    };
    w.apply(
        &Command::Strike {
            target,
            started: now,
        },
        Some(ada),
    )
    .unwrap();
    wait(w, ada, 125);
}

/// Every heap's mass (kg).
fn heaps(w: &ServerWorld) -> f64 {
    (w.items.laid.values())
        .filter(|(i, ..)| matches!(i, Item::Bulk(_)))
        .map(|(i, ..)| i.mass())
        .sum()
}

/// The ground's rock just under `at`, and its density.
fn ground(w: &ServerWorld, at: DVec3) -> (u16, f64) {
    let rock = w
        .terrain
        .rock_at(at - at.normalize() * 0.02)
        .expect("ground");
    (
        rock,
        w.terrain.planet().geology.rocks.rocks[usize::from(rock)].density,
    )
}

#[test]
fn a_picks_blow_lays_a_heap_of_the_ground_it_broke() {
    let (mut w, ada, at) = ada(true);
    let (rock, density) = ground(&w, at);
    strike(&mut w, ada, at);
    let pit = w.digging.pits.get(&spot(at)).expect("a ledger");
    let mass = heaps(&w);
    assert!((mass - density * pit.volume).abs() < 1e-12 * mass);
    // A pick's blow on soil or rock: from a fraction of a cm³ to a few litres.
    assert!(pit.volume > 1e-7 && pit.volume < 5e-3, "{} m³", pit.volume);
    let Some((Item::Bulk(b), ..)) = w.items.laid.values().next() else {
        panic!("a heap");
    };
    let rocks = &w.terrain.planet().geology.rocks;
    assert_eq!(b.composition, rocks.composition(rock, 1.0));
    // The same spot again: the same heap grows.
    strike(&mut w, ada, at);
    assert_eq!(w.items.laid.len(), 1);
    assert!(heaps(&w) > 1.9 * mass);
}

#[test]
fn ten_thousand_blows_cut_pits_and_keep_the_mass() {
    let (mut w, ada, at) = ada(true);
    // One ground throughout: a mound of soil (where the spawn's soil gives onto its bedrock, the
    // pits would cut a mix the blows did not dig: ADR-0032, Limits).
    let soil = w.terrain.planet().geology.rocks.rock("soil").unwrap();
    let up = at.normalize();
    w.terrain.edit(Edit::Fill(
        Brush::Sphere {
            center: at,
            radius: 2.5,
        },
        soil,
    ));
    let at = w.ground_under(at + up * 3.0);
    let (_, density) = ground(&w, at);
    let swing = creeer_sim::strike::swing(&w.tools[0]);
    let (east, north) = tangent_frame(up);
    let before = Field::around(&w.terrain, at, 5.0, 0.15).volume();
    let (mut slowest, mut total) = (0.0f64, 0.0);
    for k in 0..10_000u32 {
        // Four spots a metre apart, each struck where its ground now is.
        let off = if k % 2 == 0 { east } else { north } * if k % 4 < 2 { -0.5 } else { 0.5 };
        let p = w.ground_under(at + off + up * 0.5);
        let t = Instant::now();
        w.dig(ada, p, (&swing, Some(0)));
        let spent = t.elapsed().as_secs_f64();
        (slowest, total) = (slowest.max(spent), total + spent);
    }
    let owed: f64 = w.digging.pits.values().map(|p| p.volume).sum();
    assert!(w.digging.cut > 0.5, "pits were cut: {} m³", w.digging.cut);
    let mass = heaps(&w);
    let closure = mass - density * (w.digging.cut + owed);
    assert!(closure.abs() < 1e-9 * mass, "{closure} kg of {mass}");
    // The terrain measured anew on another grid lost what was cut, to its discretisation.
    let lost = before - Field::around(&w.terrain, at, 5.0, 0.15).volume();
    assert!(
        (lost / w.digging.cut - 1.0).abs() < 0.05,
        "{lost} vs {}",
        w.digging.cut
    );
    eprintln!(
        "10,000 blows: {:.2} m³ cut, {:.4} ms a strike, {:.2} ms at worst",
        w.digging.cut,
        total / 10.0,
        slowest * 1e3
    );
    // Budget: a strike 0.2 ms on average; the rare one that cuts a pit within a few ms.
    assert!(total / 1e4 < 0.2e-3, "{} ms a strike", total / 10.0);
    assert!(slowest < 5e-3, "{} ms at worst", slowest * 1e3);
}

#[test]
fn bare_hands_dig_soil_but_not_rock() {
    let (mut w, ada, at) = ada(false);
    let (rock, _) = ground(&w, at);
    let terrain = Arc::clone(&w.terrain);
    let rocks = &terrain.planet().geology.rocks;
    assert_eq!(rocks.rocks[usize::from(rock)].name, "soil", "spawn on soil");
    strike(&mut w, ada, at);
    let dug = w.digging.pits.get(&spot(at)).map_or(0.0, |p| p.volume);
    // A handful: 0.05–1 l.
    assert!(dug > 5e-5 && dug < 1e-3, "{dug} m³");
    // Granite under the hands.
    let granite = rocks.rock("granite").unwrap();
    let up = at.normalize();
    w.terrain.edit(Edit::Fill(
        Brush::Sphere {
            center: at + up * 1.0,
            radius: 1.5,
        },
        granite,
    ));
    // (Its top is out of reach: the blow is given directly.)
    let on = w.ground_under(at + up * 3.0);
    assert_eq!(w.terrain.rock_at(on - up * 0.02), Some(granite));
    let heap = heaps(&w);
    w.items.notes.clear();
    w.dig(ada, on, (&creeer_sim::dig::hands(), None));
    assert_eq!(heaps(&w), heap, "nothing dug from granite by hand");
    assert!(w.items.notes.iter().any(|(_, n)| n.contains("too hard")));
}

#[test]
fn a_stone_heads_edge_wears_and_blunts() {
    let (mut w, ada, at) = ada(false);
    let mut head = Piece::solid(
        "chert",
        2580.0,
        Form::Lump {
            size: DVec3::new(0.12, 0.07, 0.04),
        },
    );
    head.edge = Some(creeer_sim::items::Edge {
        angle: 30f64.to_radians(),
        length: 0.08,
        radius: 5e-6,
    });
    w.put_in_hands(ada, Item::Piece(head, Vec::new())).unwrap();
    wait(&mut w, ada, 60);
    let swing = creeer_sim::dig::hands();
    let granite = w.terrain.planet().geology.rocks.rock("granite").unwrap();
    let up = at.normalize();
    w.terrain.edit(Edit::Fill(
        Brush::Sphere {
            center: at + up * 1.0,
            radius: 1.5,
        },
        granite,
    ));
    let on = w.ground_under(at + up * 3.0);
    assert_eq!(w.terrain.rock_at(on - up * 0.02), Some(granite));
    for _ in 0..200 {
        w.dig(ada, on, (&swing, None));
    }
    let held = w.carried(ada).unwrap();
    let Some(Item::Piece(p, _)) = w.items.bodies.get(&held) else {
        panic!("still held");
    };
    let edge = p.edge.unwrap();
    assert!(p.wear > 0.0 && p.wear < 0.1, "{}", p.wear);
    assert!(edge.radius > 5e-6, "the edge blunted: {}", edge.radius);
}
