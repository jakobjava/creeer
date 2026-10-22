//! People in the world: a character dropped from high falls as its ragdoll, hurt, and stands up
//! again where it lies; one killed is taken away and stands whole at the spawn point; a world
//! saved with a person down loads with them down.

use super::looks::Drawn;
use super::people::{RESPAWNS_AFTER, RISES_AFTER};
use super::tests::world;
use super::*;
use creeer_sim::save::Save;

/// Ada's avatar dropped with its feet `height` above the ground where it stands.
fn dropped(height: f64) -> (ServerWorld, EntityId) {
    let mut w = world();
    let ada = w.avatar_of("ada");
    let i = w.index(ada).unwrap();
    let body = &mut w.world.bodies[i];
    let up = body.structure.position[0].normalize();
    body.structure.position[0] += up * height;
    body.solver.reset_history(&body.structure);
    (w, ada)
}

/// Steps `ticks` (Ada steered as standing still), and whether she was down in any.
fn run(w: &mut ServerWorld, ada: EntityId, ticks: u32) -> bool {
    let mut down = false;
    for _ in 0..ticks {
        w.steer(ada, &AvatarInput::default());
        w.step();
        down |= w.person(ada).unwrap().fallen();
    }
    down
}

#[test]
fn a_character_dropped_from_high_falls_hurt_and_stands_up_again() {
    let (mut w, ada) = dropped(10.0);
    assert!(run(&mut w, ada, 180), "a 10 m fall left her standing");
    let p = w.person(ada).unwrap().clone();
    assert_eq!(p.parts.len(), 15, "her ragdoll's segments");
    assert_eq!(w.world.bodies.len(), 15);
    // Her pelvis drawn as her character fallen, each other segment as hers, which (G1.3e).
    assert!(matches!(
        w.drawn(w.index(ada).unwrap()),
        Drawn::Person { fallen: true, .. }
    ));
    for (k, part) in p.parts.iter().enumerate().skip(1) {
        assert_eq!(
            w.drawn(w.index(*part).unwrap()),
            Drawn::Segment {
                person: ada,
                index: u8::try_from(k).unwrap()
            }
        );
    }
    assert!(w.take_changed().contains(&ada));
    let health = p.health;
    assert!((0.0..0.9).contains(&health), "health {health} after 10 m");
    // Still for two seconds: up again, her segments gone, healing.
    let mut ticks = 0;
    while w.person(ada).unwrap().fallen() {
        run(&mut w, ada, 1);
        ticks += 1;
        assert!(ticks < 1200, "down for 20 s");
    }
    assert!(ticks >= RISES_AFTER);
    assert_eq!(w.world.bodies.len(), 1);
    assert!(matches!(
        w.drawn(w.index(ada).unwrap()),
        Drawn::Person { fallen: false, ref player, .. } if player == "ada"
    ));
    assert!(w.take_changed().contains(&ada));
    // Standing on her legs, where she fell.
    run(&mut w, ada, 120);
    let p = w.person(ada).unwrap();
    assert!(!p.fallen(), "fell again standing up");
    assert!(p.health > health, "health {} not healing", p.health);
    let at = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let standing = creeer_sim::character::heights().0;
    let height = w.terrain.density(at);
    assert!((height - standing).abs() < 0.05, "her centre {height} m up");
}

#[test]
fn one_killed_stands_whole_at_the_spawn_point() {
    let (mut w, ada) = dropped(40.0);
    run(&mut w, ada, 240);
    assert_eq!(w.person(ada).unwrap().health, 0.0, "survived 40 m");
    assert!(w.person(ada).unwrap().fallen());
    run(&mut w, ada, RESPAWNS_AFTER + 10);
    let p = w.person(ada).unwrap();
    assert!(!p.fallen() && p.health > 0.99, "{p:?}");
    let i = w.index(ada).unwrap();
    let at = w.world.bodies[i].structure.position[0];
    assert!(
        (at - w.spawn()).length() < 2.0,
        "at {at}, the spawn {}",
        w.spawn()
    );
    let gas = w.gas(ada);
    assert!(
        (gas - creeer_sim::character::TANK).abs() < 1e-9,
        "gas {gas}"
    );
}

#[test]
fn a_world_saved_with_a_person_down_loads_with_them_down() {
    let (mut w, ada) = dropped(10.0);
    run(&mut w, ada, 120);
    let p = w.person(ada).unwrap().clone();
    assert!(p.fallen());
    let save = Save::from_bytes(&w.to_save().to_bytes()).unwrap();
    let mut back = ServerWorld::from_save(creeer_data::Data::base(), save, super::tests::terrain());
    let q = back.person(ada).unwrap();
    assert_eq!((q.health, &q.parts), (p.health, &p.parts));
    assert_eq!(back.world.joints().len(), 14);
    // Its limbs pass through each other as when it fell, but for the arms on the trunk.
    let at = |k: usize| back.index(q.parts[k]).unwrap();
    assert!(back.world.passes(at(6), at(12)), "the thighs pass");
    assert!(
        !back.world.passes(at(4), at(1)),
        "a forearm meets the torso"
    );
    // And stands up as she would have.
    let mut ticks = 0;
    while back.person(ada).unwrap().fallen() {
        run(&mut back, ada, 1);
        ticks += 1;
        assert!(ticks < 1200, "down for 20 s");
    }
    assert_eq!(back.world.bodies.len(), 1);
}
