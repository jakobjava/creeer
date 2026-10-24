//! Trees in the world (G1.7, ADR-0031): a character walks into a trunk; a tree notched on one side
//! and cut from the other goes over away from its feller, on its hinge, losing no mass; a felled
//! trunk is limbed and bucked, its mass kept; a save keeps what was done.

use super::tests::world;
use super::*;
use creeer_net::message::Target;
use creeer_sim::trees::{Standing, Tree};

/// The nearest standing tree to the spawn of 15–30 cm, grown.
pub(super) fn a_tree(w: &ServerWorld) -> (Standing, Tree) {
    let forest = Arc::clone(w.forest().expect("the packs have trees"));
    let spawn = w.spawn();
    let mut near: Vec<Standing> = (forest.cells_near(spawn, 40.0).into_iter())
        .flat_map(|c| forest.in_cell(&w.terrain, c))
        .filter(|t| (0.15..0.3).contains(&t.dbh))
        .collect();
    near.sort_by(|a, b| {
        (a.base - spawn)
            .length()
            .total_cmp(&(b.base - spawn).length())
    });
    let t = near[0];
    (t, forest.grow(&t))
}

/// Avatar `ada` stood (anew) at the ground beside tree `t`, `away` m from its axis on `side`
/// (unit, across its stem).
pub(super) fn stand(w: &mut ServerWorld, ada: EntityId, t: &Standing, side: DVec3, away: f64) {
    let feet = w.ground_under(t.base + side * away);
    put(w, ada, feet);
    wait(w, ada, 30);
}

/// Avatar `ada` moved to stand on `feet`, at rest.
pub(super) fn put(w: &mut ServerWorld, ada: EntityId, feet: DVec3) {
    let i = w.index(ada).unwrap();
    let up = feet.normalize();
    let body = &mut w.world.bodies[i];
    let s = &mut body.structure;
    s.position[0] =
        feet + up * (character::body(feet, up, DT).structure.position[0] - feet).length();
    s.orientation[0] = creeer_math::glam::DQuat::from_rotation_arc(DVec3::Y, up);
    s.velocity[0] = DVec3::ZERO;
    s.angular_velocity[0] = DVec3::ZERO;
    body.solver.reset_history(&body.structure);
    body.wake();
    w.lay_trunks_now();
}

pub(super) fn wait(w: &mut ServerWorld, ada: EntityId, n: u64) {
    for _ in 0..n {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
}

/// Swings at tree `t` at `height` until `done` or `most` blows: the blows, and the slowest
/// step (ms).
pub(super) fn chop(
    w: &mut ServerWorld,
    ada: EntityId,
    t: &Standing,
    most: u32,
    done: impl Fn(&ServerWorld) -> bool,
) -> (u32, f64) {
    let mut worst: f64 = 0.0;
    for blow in 1..=most {
        #[allow(clippy::cast_precision_loss)]
        let now = w.tick as f64;
        let strike = Command::StrikeTree {
            tree: t.id,
            height: 0.7,
            started: now,
        };
        w.apply(&strike, Some(ada)).unwrap();
        for _ in 0..120 {
            w.steer(ada, &AvatarInput::default());
            let started = std::time::Instant::now();
            w.step();
            worst = worst.max(started.elapsed().as_secs_f64() * 1e3);
        }
        if done(w) {
            return (blow, worst);
        }
    }
    panic!("not done in {most} blows");
}

/// A world with ada beside a tree, the axe in hand, and the tree with its frame's east.
pub(super) fn feller() -> (ServerWorld, EntityId, Standing, Tree, DVec3) {
    let mut w = world();
    let ada = w.avatar_of("ada");
    w.apply(&Command::Wield { tool: Some(2) }, Some(ada))
        .unwrap();
    let (t, tree) = a_tree(&w);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(t.base.normalize());
    (w, ada, t, tree, east)
}

#[test]
fn a_character_walks_into_a_trunk_and_stops() {
    let (mut w, ada, t, tree, east) = feller();
    stand(&mut w, ada, &t, east, 3.0);
    let walk = AvatarInput {
        walk: -east * 1.5,
        ..AvatarInput::default()
    };
    for _ in 0..240 {
        w.steer(ada, &walk);
        w.step();
    }
    let at = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let off = at - t.base;
    let across = (off - t.base.normalize() * off.dot(t.base.normalize())).length();
    assert!(
        across > tree.radius(1.0),
        "walked into the stem: {across:.2} m"
    );
    assert!(off.dot(east) > 0.0, "still on its side");
}

#[test]
fn notched_on_one_side_and_cut_from_the_other_it_falls_away_from_its_feller() {
    let (mut w, ada, t, tree, east) = feller();
    let r = tree.radius(0.7);
    // The notch, from the east, half through.
    stand(&mut w, ada, &t, east, r + 0.6);
    let half = |w: &ServerWorld| {
        let cut = &w.trees.cuts[&t.id];
        cut.notches(r).iter().any(|n| n.depth >= r)
    };
    let (notch, _) = chop(&mut w, ada, &t, 400, half);
    // The back cut, from the west, until it goes.
    stand(&mut w, ada, &t, -east, r + 0.6);
    let felled = |w: &ServerWorld| w.trees.felled.contains_key(&t.id);
    let (back, worst) = chop(&mut w, ada, &t, 400, felled);
    let (&id, part) = w.trees.parts.iter().next().expect("its part a body");
    let part = part.clone();
    // No mass lost: the part is all that stood above the cut; the cut's wood lies as chips.
    let i = w.index(id).unwrap();
    let mass = w.world.bodies[i].structure.total_mass();
    assert!((mass - tree.above(part.from).0).abs() < 1e-9 * mass);
    assert!(w.trees.hinged.contains(&id));
    // It falls, and lies to the east, its hinge let go.
    wait(&mut w, ada, 900);
    let s = &w.world.bodies[w.index(id).unwrap()].structure;
    let (_, centre, _) = tree.part_mass(&part);
    let top = s.position[0] + s.orientation[0] * (DVec3::new(0.0, 0.0, tree.height) - centre);
    let up = t.base.normalize();
    let lean = (s.orientation[0] * DVec3::Z).dot(up);
    println!(
        "{:.0} cm {}: notch {notch} blows, back cut {back}; worst step {worst:.1} ms; lies {:.0}° \
         from upright, its top {:.1} m east",
        tree.dbh * 100.0,
        w.forest().unwrap().species[usize::from(t.species)].name,
        creeer_math::det::acos(lean).to_degrees(),
        (top - t.base).dot(east)
    );
    assert!(w.trees.hinged.is_empty(), "the hinge let go");
    assert!(lean < 0.5, "lying down: {lean}");
    assert!(
        (top - t.base).dot(east) > 0.5 * tree.height,
        "fell to the east"
    );
}

#[test]
fn a_felled_trunk_is_bucked_and_limbed_its_mass_kept() {
    let (mut w, ada, t, tree, east) = feller();
    let r = tree.radius(0.7);
    stand(&mut w, ada, &t, east, r + 0.6);
    let felled = |w: &ServerWorld| w.trees.felled.contains_key(&t.id);
    chop(&mut w, ada, &t, 600, felled);
    wait(&mut w, ada, 900);
    let mass = |w: &ServerWorld| -> f64 {
        let parts: f64 = (w.trees.parts.keys())
            .map(|id| w.world.bodies[w.index(*id).unwrap()].structure.total_mass())
            .sum();
        let laid: f64 = w.items.laid.values().map(|(item, ..)| item.mass()).sum();
        parts + laid
    };
    let before = mass(&w);
    // Strike its stem 1.5 m from the cut, then in its crown, standing beside each.
    for (along, until) in [(1.5, 2usize), (tree.crown_base + 1.0, 0)] {
        for _ in 0..300 {
            let (&id, part) = (w.trees.parts.iter())
                .find(|(_, p)| p.from <= along + 0.7 && along + 0.7 < p.to)
                .unwrap();
            let s = &w.world.bodies[w.index(id).unwrap()].structure;
            let (_, centre, _) = tree.part_mass(part);
            let axis =
                s.position[0] + s.orientation[0] * (DVec3::new(0.0, 0.0, along + 0.7) - centre);
            let up = axis.normalize();
            let at = axis + up * tree.radius(along + 0.7);
            let side = s.orientation[0] * DVec3::Z;
            let across = up.cross(side).normalize();
            let feet = w.ground_under(axis + across * 0.9);
            put(&mut w, ada, feet);
            wait(&mut w, ada, 10);
            #[allow(clippy::cast_precision_loss)]
            let now = w.tick as f64;
            let target = Target {
                body: Some(id),
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
            wait(&mut w, ada, 120);
            let pieces = w.trees.parts.len();
            let branches = (w.items.laid.values())
                .filter(|(i, ..)| {
                    i.head().is_some_and(|p| {
                        p.material == "spruce_green"
                            || p.material == "pine_green"
                            || p.material == "birch_green"
                    })
                })
                .count();
            if (until > 0 && pieces >= until) || (until == 0 && branches > 0) {
                break;
            }
        }
    }
    assert!(w.trees.parts.len() >= 2, "bucked in two");
    let after = mass(&w);
    assert!((after - before).abs() < 1e-9 * before, "{before} {after}");
}

#[test]
fn a_save_keeps_what_was_done_to_the_trees() {
    let (mut w, ada, t, tree, east) = feller();
    stand(&mut w, ada, &t, east, tree.radius(0.7) + 0.6);
    chop(&mut w, ada, &t, 3, |_| {
        false.then_some(()).is_some() || true
    });
    let save = w.to_save();
    let back = ServerWorld::from_save(
        creeer_data::Data::base(),
        creeer_sim::save::Save::from_bytes(&save.to_bytes()).unwrap(),
        super::tests::terrain(),
    );
    assert_eq!(back.trees.cuts, w.trees.cuts);
    assert_eq!(back.trees.trunks, w.trees.trunks);
    assert_eq!(back.to_save().to_bytes(), save.to_bytes());
    assert_eq!(back.trees_since(0).0, w.trees_since(0).0);
}

#[test]
fn a_branch_carried_into_a_trunk_stays_steady_in_the_hands() {
    // (Found by a test walking a branch through the forest: it spun up to 145 rad/s.)
    let (mut w, ada, t, tree, east) = feller();
    stand(&mut w, ada, &t, east, 2.5);
    let stick = creeer_sim::items::Piece::solid(
        "spruce_green",
        800.0,
        creeer_sim::items::Form::Rod {
            radius: 0.02,
            length: 1.2,
        },
    );
    let at = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let held = w.add_item(
        creeer_sim::items::Item::Piece(stick, Vec::new()),
        at + east * 0.6,
        false,
    );
    let hold = w.world.bodies[w.index(held).unwrap()].structure.position[0];
    w.take_hold(ada, (w.index(held).unwrap(), 0), hold).unwrap();
    wait(&mut w, ada, 60);
    let walk = AvatarInput {
        walk: -east * 1.3,
        ..AvatarInput::default()
    };
    let mut most: f64 = 0.0;
    for _ in 0..300 {
        w.steer(ada, &walk);
        w.step();
        if let Some(i) = w.carried(ada).and_then(|c| w.index(c)) {
            most = most.max(w.world.bodies[i].structure.angular_velocity[0].length());
        }
    }
    let at = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    println!(
        "most spin {most:.1} rad/s; held: {}; {:.2} m from the stem",
        w.carried(ada).is_some(),
        (at - t.base).length()
    );
    assert!(most < 10.0, "spins at {most} rad/s");
    let _ = tree;
}
