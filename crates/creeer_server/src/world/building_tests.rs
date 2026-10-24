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

/// The nail pulled out of two nailed poles: the next step makes them two entities again, each
/// with its own record.
#[test]
fn two_nailed_poles_whose_nail_is_pulled_are_two_again() {
    let (mut w, ada, _, above, at) = crossed(true);
    fasten(&mut w, ada, above, at, "nail_100").expect("begins");
    wait(&mut w, ada, 400);
    let bodies = w.world.bodies.len();
    let i = w.index(above).expect("built");
    let s = &w.world.bodies[i].structure;
    let joint = (0..s.element_count())
        .find(|&e| s.member(e).is_none())
        .expect("a joint");
    w.world.bodies[i].structure.sever(joint);
    wait(&mut w, ada, 1);
    assert_eq!(w.world.bodies.len(), bodies + 1);
    assert_eq!(w.building.built.len(), 2);
    assert!(
        w.building
            .built
            .values()
            .all(|b| b.woods.iter().all(Option::is_some))
    );
    wait(&mut w, ada, 120);
}

/// Taken apart where it was nailed, after the time it takes: two poles again.
#[test]
fn a_nailed_joint_taken_apart_leaves_the_poles_apart() {
    let (mut w, ada, _, above, at) = crossed(true);
    fasten(&mut w, ada, above, at, "nail_100").expect("begins");
    wait(&mut w, ada, 400);
    let bodies = w.world.bodies.len();
    let target = Target {
        body: Some(above),
        at,
        seen: w.tick as f64,
    };
    w.apply(&Command::Unfasten(target), Some(ada))
        .expect("begins");
    wait(&mut w, ada, 590);
    assert_eq!(w.world.bodies.len(), bodies, "not before its time");
    wait(&mut w, ada, 20);
    assert_eq!(w.world.bodies.len(), bodies + 1, "{:?}", w.take_notes());
}

/// An axe cuts a pole of two nailed poles through, far from the nail: the structure falls
/// apart (the cut-off end a body of its own).
#[test]
fn an_axe_cuts_a_built_pole_through() {
    let (mut w, ada, _, above, at) = crossed(true);
    fasten(&mut w, ada, above, at, "nail_100").expect("begins");
    wait(&mut w, ada, 400);
    w.apply(&Command::Wield { tool: Some(2) }, Some(ada))
        .unwrap();
    let bodies = w.world.bodies.len();
    for _ in 0..60 {
        let i = w.index(above).expect("built");
        let s = &w.world.bodies[i].structure;
        // A point on the first pole's top, 0.5 m from its middle.
        let (a, b, thick) = s.member(0).expect("a member");
        let (pa, pb) = (s.position[a], s.position[b]);
        let up = pa.normalize();
        let target = Target {
            body: Some(above),
            at: pa + (pb - pa) * 0.5 + up * (0.5 * thick),
            seen: w.tick as f64,
        };
        let started = w.tick as f64;
        w.apply(&Command::Strike { target, started }, Some(ada))
            .unwrap();
        wait(&mut w, ada, 125);
        if w.world.bodies.len() > bodies {
            return;
        }
    }
    panic!("not cut through: {:?}", w.take_notes());
}

/// A pole stood in a hole 0.3 m deep: let go, it falls over; set (backfilled), it stands.
#[test]
fn a_pole_set_in_a_hole_stands_and_one_let_go_falls() {
    use creeer_terrain::edits::{Brush, Edit};
    for set in [false, true] {
        let mut w = world();
        w.creative = true;
        let ada = w.avatar_of("ada");
        wait(&mut w, ada, 60);
        let me = centre(&w, ada);
        let up = character::plumb(&w.world, me);
        let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
        let ground = w.ground_under(me + east * 1.5);
        w.terrain.edit(Edit::Dig(Brush::Sphere {
            center: ground,
            radius: 0.3,
        }));
        let pole = Item::Piece(
            Piece::solid(
                "spruce_green",
                800.0,
                Form::Rod {
                    radius: 0.05,
                    length: 2.0,
                },
            ),
            Vec::new(),
        );
        // Upright, its foot 0.28 m down in the hole.
        let id = w.add_item(pole, ground + up * 0.72, false);
        let i = w.index(id).expect("there");
        let s = &mut w.world.bodies[i].structure;
        s.orientation[0] = DQuat::from_rotation_arc(DVec3::X, up);
        if set {
            w.set_post(id).expect("set");
        }
        wait(&mut w, ada, 180);
        let s = &w.world.bodies[w.index(id).expect("there")].structure;
        // Its axis: a rigid pole's own, a post's from its lowest free node to its highest.
        let free: Vec<DVec3> = (0..s.node_count())
            .filter(|&n| !s.held_fast(n))
            .map(|n| s.position[n])
            .collect();
        let height = |p: &DVec3| (*p - ground).dot(up);
        let axis = if free.len() < 2 {
            s.orientation[0] * DVec3::X
        } else {
            let top = free
                .iter()
                .max_by(|a, b| height(a).total_cmp(&height(b)))
                .expect("nodes");
            let foot = free
                .iter()
                .min_by(|a, b| height(a).total_cmp(&height(b)))
                .expect("nodes");
            *top - *foot
        };
        let lean = creeer_math::det::acos(axis.normalize().dot(up).abs()).to_degrees();
        if set {
            assert!(lean < 3.0, "set, it leans {lean}°");
        } else {
            assert!(lean > 30.0, "let go, it still stands ({lean}°)");
        }
    }
}
