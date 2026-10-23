//! Making by hand in the world (G1.6d, ADR-0030): a hammerstone's blow knaps a chert core lying
//! before the player (a flake beside it, no mass lost), bark is stripped into a pocket, a head is
//! bound to the haft in the hands, and a made axe whose binding a blow overcomes comes apart.

use super::tests::world;
use super::*;
use creeer_net::message::Target;
use creeer_sim::items::{Form, Item, Piece};

fn wait(w: &mut ServerWorld, ada: EntityId, n: u32) {
    for _ in 0..n {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
}

fn centre(w: &ServerWorld, id: EntityId) -> DVec3 {
    w.world.bodies[w.index(id).unwrap()].structure.position[0]
}

/// Ada standing, and a point on the ground `metres` before her (east).
fn ada() -> (ServerWorld, EntityId, DVec3) {
    let mut w = world();
    w.creative = false;
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60);
    let at = centre(&w, ada);
    let up = character::plumb(&w.world, at);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    (w, ada, east)
}

fn lump(material: &str, density: f64, size: DVec3) -> Item {
    Item::Piece(
        Piece::solid(material, density, Form::Lump { size }),
        Vec::new(),
    )
}

/// Puts `item` on the ground 0.9 m east of ada, settled; its id.
fn put_down(w: &mut ServerWorld, ada: EntityId, east: DVec3, item: Item) -> EntityId {
    put_at(w, ada, east * 0.9, item)
}

/// Puts `item` on the ground `off` from ada, settled; its id.
fn put_at(w: &mut ServerWorld, ada: EntityId, off: DVec3, item: Item) -> EntityId {
    let at = centre(w, ada);
    let up = character::plumb(&w.world, at);
    let id = w.add_item(item, w.ground_under(at + off) + up * 0.1, true);
    wait(w, ada, 60);
    id
}

/// The top of item body `id` as seen now.
fn aim(w: &ServerWorld, id: EntityId) -> Target {
    let body = &w.world.bodies[w.index(id).unwrap()];
    let c = centre(w, id);
    let up = character::plumb(&w.world, c);
    let half = body.shapes[0].1.extent(body.structure.orientation[0], up);
    #[allow(clippy::cast_precision_loss)]
    let seen = w.tick as f64;
    Target {
        body: Some(id),
        at: c + up * half,
        seen,
    }
}

/// The mass of every item there is: bodies and pockets.
fn items_mass(w: &ServerWorld, ada: EntityId) -> f64 {
    let laid: f64 = w.items.laid.values().map(|(i, ..)| i.mass()).sum();
    w.items.bodies.values().map(Item::mass).sum::<f64>() + laid + w.load(ada)
}

/// Ada strikes `id` with what her hands hold, and the blow falls.
fn strike(w: &mut ServerWorld, ada: EntityId, id: EntityId) {
    #[allow(clippy::cast_precision_loss)]
    let now = w.tick as f64;
    let target = aim(w, id);
    w.apply(
        &Command::Strike {
            target,
            started: now,
        },
        Some(ada),
    )
    .unwrap();
    wait(w, ada, 70);
}

#[test]
fn a_hammerstones_blow_knaps_a_chert_core_and_loses_no_mass() {
    let (mut w, ada, east) = ada();
    let core = put_down(
        &mut w,
        ada,
        east,
        lump("chert", 2580.0, DVec3::new(0.14, 0.10, 0.06)),
    );
    let hammer = lump("basalt", 2900.0, DVec3::new(0.10, 0.08, 0.06));
    w.put_in_hands(ada, hammer).unwrap();
    wait(&mut w, ada, 60);
    let total = items_mass(&w, ada);
    strike(&mut w, ada, core);
    assert!(w.index(core).is_none(), "the core was broken and made anew");
    assert_eq!(w.items.laid.len(), 2, "the core and a flake lie there");
    assert!((items_mass(&w, ada) - total).abs() < 1e-9 * total);
    let edged = (w.items.laid.values().map(|(i, ..)| i))
        .filter(|i| matches!(i, Item::Piece(p, _) if p.edge.is_some()))
        .count();
    assert_eq!(edged, 2, "the flake and the core both have edges");
}

#[test]
fn bark_strips_into_a_pocket_and_a_head_binds_to_the_haft() {
    let (mut w, ada, east) = ada();
    let branch = Item::Piece(
        Piece::solid(
            "spruce_c24",
            420.0,
            Form::Rod {
                radius: 0.02,
                length: 0.7,
            },
        ),
        Vec::new(),
    );
    w.put_in_hands(ada, branch).unwrap();
    wait(&mut w, ada, 30);
    let total = items_mass(&w, ada);
    w.apply(&Command::Strip, Some(ada)).unwrap();
    assert!(w.load(ada) > 0.0, "the strip is in a pocket");
    assert!((items_mass(&w, ada) - total).abs() < 1e-9 * total);
    let head = put_down(
        &mut w,
        ada,
        east,
        lump("chert", 2580.0, DVec3::new(0.12, 0.07, 0.03)),
    );
    let total = items_mass(&w, ada);
    w.apply(&Command::Bind(aim(&w, head)), Some(ada)).unwrap();
    let held = w.carried(ada).expect("the axe in hand");
    assert!(matches!(w.items.bodies[&held], Item::Made(_)));
    assert!(w.load(ada).abs() < 1e-15, "the strip went into the binding");
    assert!((items_mass(&w, ada) - total).abs() < 1e-9 * total);
    // With nothing left to bind with, refused.
    assert!(w.apply(&Command::Bind(aim(&w, held)), Some(ada)).is_err());
}

#[test]
fn a_made_axe_whose_binding_gives_comes_apart() {
    let (mut w, ada, east) = ada();
    let crate_ = put_at(
        &mut w,
        ada,
        east * 1.5,
        lump("granite", 2650.0, DVec3::new(0.3, 0.25, 0.2)),
    );
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
    let binding = Piece::solid(
        "spruce_bast",
        600.0,
        Form::Block {
            size: DVec3::new(0.9, 0.02, 0.0015),
        },
    );
    let mut made = creeer_sim::making::bind(haft, head, binding, 20e6).unwrap();
    made.hold = 1.0; // As good as unbound.
    let parts = made.parts.len() + made.bindings.len();
    w.put_in_hands(ada, Item::Made(Box::new(made))).unwrap();
    wait(&mut w, ada, 60);
    let total = items_mass(&w, ada);
    strike(&mut w, ada, crate_);
    assert!(w.carried(ada).is_none(), "it fell apart in the hands");
    assert!(!w.items.bodies.values().any(|i| matches!(i, Item::Made(_))));
    // (The granite crumbled a little too, beside it.)
    assert!(w.items.laid.len() >= parts, "its pieces lie there");
    assert!((items_mass(&w, ada) - total).abs() < 1e-9 * total);
}

#[test]
fn a_flake_lying_where_it_fell_binds_onto_a_stick_and_a_strip_is_no_head() {
    let (mut w, ada, east) = ada();
    // A flake knapped off a chert core: it lies on the ground (laid, not a body).
    let core = put_down(
        &mut w,
        ada,
        east,
        lump("chert", 2580.0, DVec3::new(0.14, 0.10, 0.06)),
    );
    let hammer = lump("basalt", 2900.0, DVec3::new(0.10, 0.08, 0.06));
    w.put_in_hands(ada, hammer).unwrap();
    wait(&mut w, ada, 60);
    strike(&mut w, ada, core);
    let flake = (w.items.laid.iter())
        .find(
            |(_, (i, ..))| matches!(i, Item::Piece(p, _) if p.edge.is_some_and(|e| e.angle < 0.6)),
        )
        .map(|(id, _)| *id)
        .expect("a flake lies there");
    // A stick in the hands, a strip lying beside.
    w.apply(&Command::Drop, Some(ada)).unwrap();
    wait(&mut w, ada, 30);
    let stick = Piece::solid(
        "spruce_c24",
        420.0,
        Form::Rod {
            radius: 0.015,
            length: 0.6,
        },
    );
    w.put_in_hands(ada, Item::Piece(stick, Vec::new())).unwrap();
    let strip = Piece::solid(
        "spruce_bast",
        600.0,
        Form::Block {
            size: DVec3::new(0.9, 0.03, 0.0015),
        },
    );
    let at = centre(&w, ada);
    let strip = w.lay(
        Item::Piece(strip, Vec::new()),
        at + east * 0.6,
        creeer_math::glam::DQuat::IDENTITY,
    );
    wait(&mut w, ada, 30);
    // A strip is no head.
    let why = w.apply(&Command::BindLying(strip), Some(ada)).unwrap_err();
    assert!(why.contains("a head is a stone"), "{why}");
    let total = items_mass(&w, ada);
    w.apply(&Command::BindLying(flake), Some(ada)).unwrap();
    let held = w.carried(ada).expect("the tool in hand");
    assert!(matches!(w.items.bodies[&held], Item::Made(_)));
    assert!(!w.items.laid.contains_key(&flake) && !w.items.laid.contains_key(&strip));
    assert!((items_mass(&w, ada) - total).abs() < 1e-9 * total);
    let notes = w.take_notes();
    assert!(
        notes.iter().any(|(_, n)| n.starts_with("bound:")),
        "{notes:?}"
    );
    assert!(
        w.drawn_parts(held).is_some_and(|p| p.len() == 2),
        "drawn as a haft and a head"
    );
    assert!(w.inventory(ada).hands.is_some_and(|h| h.word == "tool"));
}
