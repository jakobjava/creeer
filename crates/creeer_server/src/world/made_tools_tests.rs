//! Working trees with a made tool (G2.3, G2.8, the user's play): a flake as knapping gives it (a
//! fifth as thick as long: 20 mm for a 10 cm one, thicker than a 36 mm haft's radius), split-hafted
//! and bound with bast, cuts a standing tree and a felled stem, and its binding holds.

use super::trees_tests::{chop, feller, put, stand, wait};
use super::*;
use creeer_net::message::Target;
use creeer_sim::items::{Assembly, Edge, Form, Item, Piece};

/// A 10 cm chert flake as knapping gives it, in a 36 mm green spruce haft's split end, bound
/// with bast (green spruce across its grain), as the game binds it.
fn made_axe() -> Assembly {
    let haft = Piece::solid(
        "spruce_green",
        800.0,
        Form::Rod {
            radius: 0.018,
            length: 0.6,
        },
    );
    let mut flake = Piece::solid(
        "chert",
        2580.0,
        Form::Lump {
            size: DVec3::new(0.10, 0.08, 0.02),
        },
    );
    flake.edge = Some(Edge {
        angle: 40.0,
        length: 0.05,
        radius: 2e-5,
    });
    let bast = Piece::solid(
        "spruce_bast",
        600.0,
        Form::Block {
            size: DVec3::new(0.9, 0.02, 0.0015),
        },
    );
    creeer_sim::making::bind_split(haft, flake, bast, 20e6, Some(1.8e6)).expect("binds")
}

/// Struck at a standing tree, a made axe holds together (every blow broke one off its haft: the
/// flake was thicker than the haft's radius, so only lashed on).
#[test]
fn a_made_axe_cuts_a_standing_tree_and_holds_together() {
    let (mut w, ada, t, tree, east) = feller();
    stand(&mut w, ada, &t, east, tree.radius(0.7) + 0.6);
    w.apply(&Command::Wield { tool: None }, Some(ada)).unwrap();
    w.put_in_hands(ada, Item::Made(Box::new(made_axe())))
        .expect("in the hands");
    wait(&mut w, ada, 60);
    for _ in 0..5 {
        #[allow(clippy::cast_precision_loss)]
        let now = w.tick as f64;
        let strike = Command::StrikeTree {
            tree: t.id,
            height: 0.7,
            started: now,
        };
        w.apply(&strike, Some(ada)).unwrap();
        wait(&mut w, ada, 120);
        assert!(
            w.carried(ada).is_some(),
            "the made axe came apart: {:?}",
            w.take_notes()
        );
    }
}

/// A felled stem cut through with a made axe: its binding holds (an edge cutting in stops over
/// the depth it cuts, not in a millisecond: taken so, every blow on a felled part broke it).
#[test]
fn a_made_axe_cuts_a_felled_stem_through_and_holds_together() {
    let (mut w, ada, t, tree, east) = feller();
    stand(&mut w, ada, &t, east, tree.radius(0.7) + 0.6);
    let felled = |w: &ServerWorld| w.trees.felled.contains_key(&t.id);
    chop(&mut w, ada, &t, 600, felled);
    wait(&mut w, ada, 900);
    w.apply(&Command::Wield { tool: None }, Some(ada)).unwrap();
    let mut made = Some(made_axe());
    let along = tree.crown_base + 1.0;
    // (Through, measured: 221 blows, the binding holding all the while; 20 here for CI.)
    for _ in 0..20 {
        let Some((&id, part)) =
            (w.trees.parts.iter()).find(|(_, p)| p.from <= along && along < p.to)
        else {
            break;
        };
        let s = &w.world.bodies[w.index(id).unwrap()].structure;
        let (_, centre, _) = tree.part_mass(part);
        let axis = s.position[0] + s.orientation[0] * (DVec3::new(0.0, 0.0, along) - centre);
        let up = axis.normalize();
        let side = s.orientation[0] * DVec3::Z;
        let feet = w.ground_under(axis + up.cross(side).normalize() * 0.9);
        // Beside it once, then the tool in the hands (a carried thing stays where it was).
        if let Some(m) = made.take() {
            put(&mut w, ada, feet);
            wait(&mut w, ada, 30);
            w.put_in_hands(ada, Item::Made(Box::new(m)))
                .expect("in the hands");
            wait(&mut w, ada, 60);
        }
        wait(&mut w, ada, 10);
        #[allow(clippy::cast_precision_loss)]
        let now = w.tick as f64;
        let target = Target {
            body: Some(id),
            at: axis + up * tree.radius(along),
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
        assert!(
            w.carried(ada).is_some(),
            "the made axe came apart: {:?}",
            w.take_notes()
        );
    }
    let notes = w.take_notes();
    assert!(
        notes.iter().any(|(_, n)| n.contains("% through")),
        "it cut nothing: {notes:?}"
    );
}

/// The whole of it as a player does it (the user's play, G2.8): a chert core knapped with a
/// hammerstone, a branch stripped, the sharpest flake bound on, and a standing tree struck.
#[test]
fn a_tool_made_by_hand_cuts_a_standing_tree_and_holds_together() {
    let (mut w, ada, t, tree, east) = feller();
    stand(&mut w, ada, &t, east, tree.radius(0.7) + 0.6);
    w.apply(&Command::Wield { tool: None }, Some(ada)).unwrap();
    let feet = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let up = feet.normalize();
    let side = east.cross(up).normalize();
    let ground = |w: &ServerWorld, d: DVec3| w.ground_under(feet + d);
    let turn = creeer_math::glam::DQuat::from_rotation_arc(DVec3::Z, up);
    let lump = |m: &str, rho: f64, s: DVec3| {
        Item::Piece(Piece::solid(m, rho, Form::Lump { size: s }), Vec::new())
    };
    let at = ground(&w, side * 0.5);
    w.lay(
        lump("chert", 2580.0, DVec3::new(0.14, 0.10, 0.06)),
        at,
        turn,
    );
    let at = ground(&w, -side * 0.5);
    let hammer = w.lay(
        lump("basalt", 2900.0, DVec3::new(0.10, 0.08, 0.06)),
        at,
        turn,
    );
    let branch = Piece::solid(
        "spruce_green",
        800.0,
        Form::Rod {
            radius: 0.015,
            length: 0.6,
        },
    );
    let at = ground(&w, -east * 0.4);
    let stick = w.lay(Item::Piece(branch, Vec::new()), at, turn);
    let pick = |w: &mut ServerWorld, loose: u64| {
        w.apply(
            &Command::Pick {
                loose,
                pocket: false,
            },
            Some(ada),
        )
        .unwrap();
        wait(w, ada, 30);
    };
    let chert = |w: &ServerWorld| -> Vec<u64> {
        let mut c: Vec<(u64, f64)> = (w.items.laid.iter())
            .filter(|(_, l)| matches!(&l.0, Item::Piece(p, _) if p.material == "chert"))
            .map(|(k, l)| (*k, l.0.mass()))
            .collect();
        c.sort_by(|a, b| b.1.total_cmp(&a.1));
        c.into_iter().map(|c| c.0).collect()
    };
    pick(&mut w, hammer);
    for _ in 0..3 {
        let core = chert(&w)[0];
        #[allow(clippy::cast_precision_loss)]
        let started = w.tick as f64;
        w.apply(
            &Command::StrikeLoose {
                loose: core,
                started,
            },
            Some(ada),
        )
        .unwrap();
        wait(&mut w, ada, 130);
    }
    w.apply(&Command::Drop, Some(ada)).unwrap();
    wait(&mut w, ada, 30);
    pick(&mut w, stick);
    w.apply(&Command::Strip, Some(ada)).unwrap();
    wait(&mut w, ada, 30);
    let sharp = |w: &ServerWorld, k: u64| matches!(&w.items.laid[&k].0, Item::Piece(p, _) if p.edge.is_some_and(|e| e.angle < 0.8));
    let flake = (chert(&w).into_iter()).find(|&k| sharp(&w, k));
    let flake = flake.unwrap_or_else(|| panic!("no flake: {:?}", w.take_notes()));
    w.apply(&Command::BindLying(flake), Some(ada)).unwrap();
    wait(&mut w, ada, 30);
    println!("{:?}", w.take_notes());
    for _ in 0..5 {
        #[allow(clippy::cast_precision_loss)]
        let now = w.tick as f64;
        let strike = Command::StrikeTree {
            tree: t.id,
            height: 0.7,
            started: now,
        };
        w.apply(&strike, Some(ada)).unwrap();
        wait(&mut w, ada, 120);
        let notes = w.take_notes();
        println!("{notes:?}");
        assert!(
            w.carried(ada).is_some(),
            "the made axe came apart: {notes:?}"
        );
        // Its flake cuts (its haft's blunt end struck, bruising the bark, G2.8).
        let cut = notes.iter().any(|(_, n)| n.contains("notch"));
        assert!(cut, "it cut nothing: {notes:?}");
    }
}
