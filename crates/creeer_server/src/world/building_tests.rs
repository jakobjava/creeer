//! Building by hand in the world (G2.3, ADR-0033): two poles laid across each other are
//! fastened where they cross, after the time it takes, into one structure with a joint; in
//! survival, nails are refused and a lashing takes a strip from the pockets.

use super::tests::world;
use super::*;
use creeer_math::glam::DQuat;
use creeer_net::message::Target;
use creeer_sim::items::{Form, Item, Piece};

fn wait(w: &mut ServerWorld, ada: EntityId, n: u32) {
    for _ in 0..n {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
}

fn centre(w: &ServerWorld, id: EntityId) -> DVec3 {
    w.world.bodies[w.index(id).expect("there")]
        .structure
        .position[0]
}

/// A green spruce pole, 50 mm round and 1.6 m long.
fn pole() -> Item {
    Item::Piece(
        Piece::solid(
            "spruce_green",
            800.0,
            Form::Rod {
                radius: 0.025,
                length: 1.6,
            },
        ),
        Vec::new(),
    )
}

/// Ada, and two poles crossing on the ground 1.2 m east of her, the second on the first:
/// their ids and where they cross.
fn crossed(creative: bool) -> (ServerWorld, EntityId, EntityId, EntityId, DVec3) {
    let mut w = world();
    w.creative = creative;
    let ada = w.avatar_of("ada");
    wait(&mut w, ada, 60);
    let me = centre(&w, ada);
    let up = character::plumb(&w.world, me);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    let ground = w.ground_under(me + east * 1.2);
    let below = w.add_item(pole(), ground + up * 0.03, true);
    let above = w.add_item(pole(), ground + up * 0.09, true);
    // The second turned a quarter about the up: across the first.
    let i = w.index(above).expect("there");
    let s = &mut w.world.bodies[i].structure;
    s.orientation[0] = DQuat::from_axis_angle(up, std::f64::consts::FRAC_PI_2) * s.orientation[0];
    wait(&mut w, ada, 90);
    // Where they touch: the upper one's contact with the lower.
    let j = w.index(below).expect("there");
    let touch = (w
        .world
        .touching(w.index(above).expect("there"), 0.005)
        .into_iter())
    .find(|t| t.other.is_some_and(|(k, _)| k == j))
    .expect("they cross");
    (w, ada, below, above, touch.point)
}

fn fasten(
    w: &mut ServerWorld,
    ada: EntityId,
    part: EntityId,
    at: DVec3,
    fastener: &str,
) -> Result<(), String> {
    let target = Target {
        body: Some(part),
        at,
        seen: w.tick as f64,
    };
    w.apply(
        &Command::Fasten {
            target,
            fastener: fastener.into(),
        },
        Some(ada),
    )
}

#[test]
#[ignore = "G2.3: needs face patches on multi-node structures"]
fn two_crossed_poles_nailed_become_one_structure() {
    let (mut w, ada, below, above, at) = crossed(true);
    let bodies = w.world.bodies.len();
    fasten(&mut w, ada, above, at, "nail_100").expect("begins");
    // Not before its time (6 s).
    wait(&mut w, ada, 300);
    assert!(w.index(below).is_some(), "fastened too soon");
    wait(&mut w, ada, 70);
    assert!(
        w.index(below).is_none(),
        "the second part is now the first's: {:?}",
        w.take_notes()
    );
    assert_eq!(w.world.bodies.len(), bodies - 1);
    let s = &w.world.bodies[w.index(above).expect("built")].structure;
    let members = (0..s.element_count())
        .filter(|&e| s.member(e).is_some())
        .count();
    assert_eq!(s.element_count() - members, 1, "one joint");
    assert!(
        members >= 4,
        "both poles as members, split where they cross: {members}"
    );
    // Two 1.6 m poles of 50 mm green spruce: 2.5 kg each.
    let mass = 2.0 * 800.0 * 1.6 * std::f64::consts::PI * 0.025 * 0.025;
    assert!(
        (s.total_mass() / mass - 1.0).abs() < 1e-9,
        "{}",
        s.total_mass()
    );
    // It settles where it lies, whole.
    wait(&mut w, ada, 240);
    let s = &w.world.bodies[w.index(above).expect("built")].structure;
    let fastest = s.velocity.iter().map(|v| v.length()).fold(0.0, f64::max);
    assert!(fastest < 0.01, "still moving at {fastest} m/s");
    assert!(s.elements_broken() == 0, "nothing broke");
}

#[test]
fn in_survival_nails_are_refused_and_a_lashing_needs_a_strip() {
    let (mut w, ada, _, above, at) = crossed(false);
    let nails = fasten(&mut w, ada, above, at, "nail_100");
    assert!(nails.is_err_and(|e| e.contains("nails")), "nails refused");
    let lashing = fasten(&mut w, ada, above, at, "bast_lashing");
    assert!(
        lashing.is_err_and(|e| e.contains("strip")),
        "no strip in the pockets"
    );
}

#[test]
fn a_pole_alone_has_nothing_to_be_fastened_to() {
    let (mut w, ada, below, above, _) = crossed(true);
    let far = centre(&w, above) + DVec3::X * 0.7;
    let _ = below;
    assert!(fasten(&mut w, ada, above, far, "nail_100").is_err());
}
