//! Fastened to standing trees (G2.9, ADR-0033): a pole leant against a trunk like a ladder and
//! nailed there carries a load at its top; overloaded, the nail lets go and the pole falls; the
//! tree felled, it lets go; a save keeps it; taken apart, it lets go; and in survival a player
//! lashes one to a tree aiming at the trunk, with a strip from the pockets.

use super::tests::world;
use super::trees_tests::{a_tree, put, wait};
use super::*;
use creeer_building::fasteners;
use creeer_building::law::Placement;
use creeer_math::glam::DQuat;
use creeer_net::message::Target;
use creeer_physics::world::shape::StaticShape;
use creeer_sim::items::{Form, Item, Piece};
use creeer_sim::trees::{Cut, Standing, Tree};

/// A green spruce pole, 50 mm round, `length` m long.
fn pole(length: f64) -> Item {
    Item::Piece(
        Piece::solid(
            "spruce_green",
            800.0,
            Form::Rod {
                radius: 0.025,
                length,
            },
        ),
        Vec::new(),
    )
}

/// A world (creative or not) with a tree, a pole leant against its trunk's face from 1 m out,
/// its top 1.6 m up, settled, and ada beside it: the tree, the pole, and where its top touches.
fn leant(creative: bool) -> (ServerWorld, EntityId, Standing, EntityId, DVec3) {
    let mut w = world();
    w.creative = creative;
    let ada = w.avatar_of("ada");
    let (t, _) = a_tree(&w);
    let species = &w.forest().expect("trees").species[usize::from(t.species)];
    let stem = Tree::stem_only(species, t.dbh);
    let StaticShape::Box { half, .. } = stem.trunk_boxes(t.base, t.turn, stem.height)[0] else {
        panic!("a trunk is boxes");
    };
    let (out, side, up) = (t.turn * DVec3::X, t.turn * DVec3::Y, t.turn * DVec3::Z);
    let feet = w.ground_under(t.base + out * (half.x + 0.9) + side * 0.7);
    put(&mut w, ada, feet);
    let top = t.base + out * (half.x + 0.03) + up * 1.6;
    let ground = w.ground_under(t.base + out * (half.x + 1.0));
    let foot = ground + ground.normalize() * 0.03;
    let along = top - foot;
    let id = w.add_item(pole(along.length()), foot + along * 0.5, false);
    let i = w.index(id).expect("there");
    let body = &mut w.world.bodies[i];
    body.structure.orientation[0] = DQuat::from_rotation_arc(DVec3::X, along.normalize());
    body.solver.reset_history(&body.structure);
    w.lay_trunks_now();
    wait(&mut w, ada, 120);
    let i = w.index(id).expect("there");
    let s = &w.world.bodies[i].structure;
    // Its top, on its axis 3 cm from its end (an aim at the pole there).
    let top = s.position[0] + s.orientation[0] * DVec3::X * (0.5 * along.length() - 0.03);
    let height = (top - t.base).dot(up);
    assert!(
        height > 1.5,
        "it still leans on the trunk: its top {height:.2} m up"
    );
    (w, ada, t, id, top)
}

fn aim(w: &ServerWorld, body: Option<EntityId>, at: DVec3) -> Target {
    #[allow(clippy::cast_precision_loss)]
    let seen = w.tick as f64;
    Target { body, at, seen }
}

fn fasten(w: &mut ServerWorld, ada: EntityId, target: Target, fastener: &str) {
    let fastener = fastener.into();
    (w.apply(&Command::Fasten { target, fastener }, Some(ada))).expect("begins");
}

/// The pole's joint to a tree, if it has one: its element and the pole's node at it.
fn tree_joint(w: &ServerWorld, id: EntityId) -> Option<(usize, usize)> {
    let s = &w.world.bodies[w.index(id)?].structure;
    (0..s.element_count())
        .filter_map(|e| s.joint_ends(e).map(|(a, b)| (e, a, b)))
        .find(|(_, a, _)| s.held_fast(*a) && w.tree_holding(s.position[*a]).is_some())
        .map(|(e, _, b)| (e, b))
}

/// Leant and nailed: the world, ada, the tree, the pole, where it was nailed, and its top node.
fn nailed() -> (ServerWorld, EntityId, Standing, EntityId, DVec3, usize) {
    let (mut w, ada, t, id, at) = leant(true);
    let target = aim(&w, Some(id), at);
    fasten(&mut w, ada, target, "nail_100");
    wait(&mut w, ada, 400);
    let (_, top) = tree_joint(&w, id).unwrap_or_else(|| panic!("nailed: {:?}", w.take_notes()));
    (w, ada, t, id, at, top)
}

fn position(w: &ServerWorld, id: EntityId, node: usize) -> DVec3 {
    w.world.bodies[w.index(id).expect("there")]
        .structure
        .position[node]
}

#[test]
fn a_pole_nailed_to_a_tree_carries_a_load_at_its_top() {
    let (mut w, ada, t, id, _, top) = nailed();
    let before = position(&w, id, top);
    // 50 kg down on its top, and 20 kg's worth along the trunk's face.
    let i = w.index(id).expect("built");
    let (side, up) = (t.turn * DVec3::Y, t.turn * DVec3::Z);
    w.world.bodies[i].structure.load[top] = -up * 490.0 + side * 196.0;
    w.world.bodies[i].wake();
    wait(&mut w, ada, 180);
    let moved = (position(&w, id, top) - before).length();
    assert!(moved < 0.02, "its top moved {moved} m");
    let s = &w.world.bodies[w.index(id).expect("built")].structure;
    assert_eq!(s.elements_broken(), 0, "nothing broke");
    assert!(tree_joint(&w, id).is_some(), "still nailed");
    // The nail's law, as the joint was made (its length about the stem's radius and the pole's).
    let wood = &w.forest().expect("trees").species[usize::from(t.species)].wood;
    let nail = (w.building.fasteners.iter())
        .find(|f| f.name == "nail_100")
        .expect("a nail");
    let r = w.forest().expect("trees").grow(&t).radius(1.6);
    let sides = (
        fasteners::side(w.wood("spruce_green").expect("wood"), 0.05).expect("a side"),
        fasteners::side(w.wood(wood).expect("wood"), 2.0 * r).expect("a side"),
    );
    let at = Placement {
        count: 1,
        spread: 0.0,
        contact: 0.05 * 0.05,
    };
    let materials = w.building.materials.as_ref().expect("materials");
    let law = (nail.law(materials, (&sides.0, &sides.1), &at, r + 0.025)).expect("a law");
    println!(
        "nail_100 into {wood}: slip modulus {:.0} kN/m, yield {:.0} N, peak {:.0} N, withdrawal \
         {:.0} N; its top moved {:.1} mm",
        law.lateral * 1e-3,
        law.lateral_yield,
        law.lateral_peak,
        law.withdrawal,
        moved * 1e3
    );
    // Not nailed, the same push along the face slides it off.
    let (mut w, ada, t, id, top) = leant(true);
    let i = w.index(id).expect("there");
    w.world.bodies[i].structure.load[0] = t.turn * DVec3::Y * 196.0;
    wait(&mut w, ada, 180);
    let s = &w.world.bodies[w.index(id).expect("there")].structure;
    let now = s.position[0] + s.orientation[0] * DVec3::X * ((top - s.position[0]).length());
    assert!((now - top).length() > 0.1, "it held without a nail");
}

#[test]
fn overloaded_the_nail_lets_go_and_the_pole_falls() {
    let (mut w, ada, t, id, _, top) = nailed();
    let before = position(&w, id, top);
    let i = w.index(id).expect("built");
    // 2.5 kN along the trunk's face: nearly twice the 4 mm nail's peak (1.4 kN in green birch).
    w.world.bodies[i].structure.load[top] = t.turn * DVec3::Y * 2500.0;
    w.world.bodies[i].wake();
    let mut gone = false;
    for _ in 0..120 {
        wait(&mut w, ada, 1);
        if tree_joint(&w, id).is_none() {
            gone = true;
            // The load taken off once it let go: the pole falls by its weight.
            let i = w.index(id).expect("the pole");
            let s = &mut w.world.bodies[i].structure;
            s.load.iter_mut().for_each(|l| *l = DVec3::ZERO);
            break;
        }
    }
    assert!(gone, "the nail held 2.5 kN");
    // The 2.5 kN until the nail lets go flings the pole up to 5–6 m at about 20 m/s (with
    // every build): after two seconds it was still in the air, its top 0.7 m up and falling, or
    // 2.1 m (ADR-0024, "G2.0: at rest"). Five seconds see it down.
    wait(&mut w, ada, 300);
    let s = &w.world.bodies[w.index(id).expect("the pole")].structure;
    let lowest = (s.position.iter()).map(|p| (*p - t.base).dot(t.turn * DVec3::Z));
    let highest = lowest.fold(f64::NEG_INFINITY, f64::max);
    let up = (before - t.base).dot(t.turn * DVec3::Z);
    assert!(
        highest < up - 0.3,
        "it fell: its top {highest:.2} m up, {up:.2} before"
    );
}

#[test]
fn felling_the_tree_lets_the_pole_go_and_it_falls() {
    let (mut w, ada, t, id, _, top) = nailed();
    let up = t.turn * DVec3::Z;
    let height = (position(&w, id, top) - t.base).dot(up);
    let grown = w.trees.forest.as_ref().expect("trees").grow(&t);
    // Cut through at 0.5 m, nothing left to hinge on.
    w.fell(t.id, (&t, &grown), &Cut::at(0.5), None);
    assert!(tree_joint(&w, id).is_none(), "let go at once");
    // It stood balanced on its stump: pushed off away from the pole, it goes over.
    let (&trunk, _) = w.trees.parts.iter().next().expect("the trunk a body");
    let i = w.index(trunk).expect("there");
    let body = &mut w.world.bodies[i];
    body.structure.velocity[0] = -(t.turn * DVec3::X) * 1.5;
    body.solver.reset_history(&body.structure);
    body.wake();
    wait(&mut w, ada, 180);
    let s = &w.world.bodies[w.index(id).expect("the pole")].structure;
    let highest = (s.position.iter())
        .map(|p| (*p - t.base).dot(up))
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        highest < height - 0.5,
        "it fell: {highest:.2} m, {height:.2} before"
    );
}

#[test]
fn a_save_keeps_a_pole_nailed_to_a_tree() {
    let (w, ada, _, id, _, top) = nailed();
    let save = w.to_save();
    let mut back = ServerWorld::from_save(
        creeer_data::Data::base(),
        creeer_sim::save::Save::from_bytes(&save.to_bytes()).expect("reads"),
        super::tests::terrain(),
    );
    assert_eq!(back.to_save().to_bytes(), save.to_bytes());
    assert_eq!(tree_joint(&back, id), tree_joint(&w, id));
    let before = position(&back, id, top);
    back.lay_trunks_now();
    wait(&mut back, ada, 120);
    assert!(
        (position(&back, id, top) - before).length() < 0.02,
        "still held"
    );
    assert!(tree_joint(&back, id).is_some());
}

#[test]
fn unfastened_at_the_trunk_the_pole_lets_go() {
    let (mut w, ada, t, id, _, top) = nailed();
    // Aimed at the trunk beside its top (no body): the stem's round side facing it.
    let p = position(&w, id, top);
    let up = t.turn * DVec3::Z;
    let axis = t.base + up * (p - t.base).dot(up);
    let r = w
        .forest()
        .expect("trees")
        .grow(&t)
        .radius((p - t.base).dot(up));
    let at = axis + (p - axis).normalize() * r;
    w.apply(&Command::Unfasten(aim(&w, None, at)), Some(ada))
        .expect("begins");
    wait(&mut w, ada, 590);
    assert!(tree_joint(&w, id).is_some(), "not before its time");
    wait(&mut w, ada, 20);
    assert!(tree_joint(&w, id).is_none(), "{:?}", w.take_notes());
}

/// As a player would in survival: strip bark into a pocket, put the branch down, aim at the
/// trunk where the pole leans on it, and lash. Nails are refused; the lashing takes the strip.
#[test]
fn in_survival_a_player_lashes_a_pole_to_a_tree_aiming_at_the_trunk() {
    let (mut w, ada, _, id, top) = leant(false);
    // Where the crosshair meets the trunk beside the pole's top.
    let (_, at) = w.touching_tree(id, top).expect("it touches the tree");
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
    w.put_in_hands(ada, branch).expect("in hand");
    w.apply(&Command::Strip, Some(ada)).expect("stripped");
    w.apply(&Command::Drop, Some(ada)).expect("put down");
    wait(&mut w, ada, 30);
    let target = aim(&w, None, at);
    let nails = Command::Fasten {
        target,
        fastener: "nail_100".into(),
    };
    assert!(
        w.apply(&nails, Some(ada))
            .is_err_and(|e| e.contains("nails"))
    );
    fasten(&mut w, ada, target, "bast_lashing");
    assert!(tree_joint(&w, id).is_none(), "not before its time");
    wait(&mut w, ada, 30 * 60 + 10);
    assert!(tree_joint(&w, id).is_some(), "lashed: {:?}", w.take_notes());
    assert!(
        w.find_pocket_strip(ada).is_none(),
        "the strip went into the lashing"
    );
    let items = &w.building.built[&id].items;
    assert_eq!(items.len(), 2, "the pole and the strip: {items:?}");
}
