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
    // Timed, not gated here: shared CI machines are slow and noisy. The budgets (0.2 ms a
    // strike, the rare cut within the tick's) are measured on the development machine
    // (ADR-0032, Cost; `creeer_tools play --gate` on *quarry*).
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

#[test]
fn a_heap_runs_through_the_fingers_and_is_scooped_a_pocketful_at_a_time() {
    let (mut w, ada, at) = ada(true);
    let swing = creeer_sim::strike::swing(&w.tools[0]);
    for _ in 0..40 {
        w.dig(ada, at, (&swing, Some(0)));
    }
    let (&heap, (Item::Bulk(b), ..)) = w.items.laid.iter().next().expect("a heap") else {
        panic!("a heap of loose matter");
    };
    let before = b.mass;
    assert!(w.pick(ada, heap, false).is_err(), "not in the hands");
    w.pick(ada, heap, true).unwrap();
    let left = w.items.laid.get(&heap).map_or(0.0, |(i, ..)| i.mass());
    let pocketed = w.load(ada);
    assert!(
        pocketed > 0.0 && left > 0.0,
        "a pocketful taken, the rest lying"
    );
    assert!((pocketed + left - before).abs() < 1e-9 * before);
}

#[test]
fn a_made_tool_is_held_in_one_hand_at_the_side() {
    let (mut w, ada, _) = ada(false);
    let haft = Piece::solid(
        "spruce_c24",
        420.0,
        Form::Rod {
            radius: 0.015,
            length: 0.6,
        },
    );
    let head = Piece::solid(
        "chert",
        2580.0,
        Form::Lump {
            size: DVec3::new(0.12, 0.07, 0.03),
        },
    );
    let strip = Piece::solid(
        "spruce_bast",
        600.0,
        Form::Block {
            size: DVec3::new(0.6, 0.047, 0.0015),
        },
    );
    let made = creeer_sim::making::bind(haft, head, strip, 20e6).unwrap();
    w.put_in_hands(ada, Item::Made(Box::new(made))).unwrap();
    wait(&mut w, ada, 120);
    assert!(w.carrying[&ada].one_hand);
    // Its balance at the right hand hanging at the side.
    let i = w.index(ada).unwrap();
    let centre = w.world.bodies[i].structure.position[0];
    let up = character::plumb(&w.world, centre);
    let c = &w.carrying[&ada];
    let l = &w.world.bodies[w.index(c.load).unwrap()].structure;
    let grip = l.position[0] + l.orientation[0] * c.grip;
    let side = creeer_sim::carry::at_side(centre, up, c.facing);
    assert!(
        (grip - side).length() < 0.1,
        "{:.3} m from the hand",
        (grip - side).length()
    );
    // A blow into soil does not tear its binding (a strip as stripping makes one: 2.8 kN; the
    // blow's impulse over 2 ms, as on stone, is 3.8 kN: it tore at every blow before).
    let held = w.items.bodies[&c.load].swinging().unwrap();
    let swing = creeer_sim::strike::swing_of(held.mass, held.gyration, held.carry, held.strike);
    let at = w.ground_under(centre + c.facing * 1.2);
    let hold = match &w.items.bodies[&c.load] {
        Item::Made(a) => a.hold,
        _ => 0.0,
    };
    let energy = 0.5 * swing.mass * swing.speed * swing.speed;
    let rigid = 2.0 * energy / swing.speed / 0.002;
    assert!(
        hold < rigid,
        "as stone, it would tear: {hold:.0} N, {rigid:.0} N"
    );
    for _ in 0..20 {
        w.dig(ada, at, (&swing, None));
    }
    assert!(w.carried(ada).is_some(), "it held together in soil");
}
