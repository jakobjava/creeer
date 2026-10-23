//! What a player reaches for is judged as they saw it (G1.4d): within reach of the hands, on
//! what they name as it was when they saw it, in sight past the ground; and the sandbox's
//! commands are a creative server's alone.

use super::reach::Reached;
use super::tests::world;
use super::*;
use creeer_sim::reach::{HISTORY, REACH, hands};
use creeer_terrain::edits::Brush;

/// A world with ada standing (a second steered), her avatar, its centre and up, and the
/// ground's way east from her.
fn standing() -> (ServerWorld, EntityId, DVec3, DVec3, DVec3) {
    let mut w = world();
    let ada = w.avatar_of("ada");
    for _ in 0..60 {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
    let centre = w.world.bodies[w.index(ada).unwrap()].structure.position[0];
    let up = character::plumb(&w.world, centre);
    let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
    (w, ada, centre, up, east)
}

/// Steps the world `n` ticks, ada standing.
fn wait(w: &mut ServerWorld, ada: EntityId, n: u32) {
    for _ in 0..n {
        w.steer(ada, &AvatarInput::default());
        w.step();
    }
}

#[test]
fn the_ground_is_reached_within_two_and_a_half_metres_of_the_hands() {
    let (w, ada, centre, up, east) = standing();
    let near = w.ground_under(centre + east * 2.0);
    let reached = w
        .reached(ada, None, near, w.tick as f64)
        .expect("the ground 2 m ahead");
    assert_eq!(
        reached,
        Reached {
            body: None,
            point: near
        }
    );
    let far = w.ground_under(centre + east * 3.0);
    assert!((far - hands(centre, up)).length() > REACH);
    let why = w.reached(ada, None, far, w.tick as f64).unwrap_err();
    assert!(why.contains("beyond reach"), "{why}");
    // Half a metre over the ground is not on it.
    let why = w
        .reached(ada, None, near + up * 0.5, w.tick as f64)
        .unwrap_err();
    assert!(why.contains("not on the ground"), "{why}");
    // Not finite, or no such avatar.
    assert!(w.reached(ada, None, DVec3::NAN, w.tick as f64).is_err());
    assert!(w.reached(EntityId(999), None, near, w.tick as f64).is_err());
}

#[test]
fn a_falling_crate_is_reached_where_it_was_seen_and_found_where_it_is() {
    let (mut w, ada, centre, up, east) = standing();
    // A crate dropped beside her, falling.
    w.apply(&Command::Crate(centre + east * 1.5 + up * 1.2), None)
        .unwrap();
    let id = *w.ids().last().unwrap();
    wait(&mut w, ada, 10);
    let seen = w.tick;
    let s = &w.world.bodies[w.index(id).unwrap()].structure;
    // The middle of its top, as seen.
    let faces = [
        DVec3::X,
        DVec3::Y,
        DVec3::Z,
        DVec3::NEG_X,
        DVec3::NEG_Y,
        DVec3::NEG_Z,
    ];
    let out = (faces.map(|f| s.orientation[0] * f).into_iter())
        .max_by(|a, b| a.dot(up).total_cmp(&b.dot(up)))
        .unwrap();
    let at = s.position[0] + out * 0.5;
    // Six ticks on (a client drawing 4 ticks behind, its command 2 ticks on the way): it has
    // fallen on, and the point with it.
    wait(&mut w, ada, 6);
    let reached = w
        .reached(ada, Some(id), at, seen as f64)
        .expect("the crate as seen");
    let now = &w.world.bodies[w.index(id).unwrap()].structure;
    assert_eq!(reached.body, Some((w.index(id).unwrap(), 0)));
    let fell = (at - reached.point).dot(up);
    assert!(fell > 0.05, "fallen {fell} m since seen");
    let local = now.orientation[0].conjugate() * (reached.point - now.position[0]);
    assert!(
        (local.abs().max_element() - 0.5).abs() < 1e-9,
        "on its face: {local}"
    );
    // The same point now is off the crate: not where it is.
    let why = w.reached(ada, Some(id), at, w.tick as f64).unwrap_err();
    assert!(why.contains("not on the body"), "{why}");
    // A point 0.3 m off it as seen, or seen longer ago than the history keeps.
    let off = at + out * 0.3;
    assert!(w.reached(ada, Some(id), off, seen as f64).is_err());
    wait(&mut w, ada, HISTORY);
    let why = w.reached(ada, Some(id), at, seen as f64).unwrap_err();
    assert!(why.contains("ago"), "{why}");
}

#[test]
fn a_body_far_from_every_player_is_not_reached() {
    let (mut w, ada, centre, up, east) = standing();
    w.apply(&Command::Crate(centre + east * 30.0 + up * 0.6), None)
        .unwrap();
    let id = *w.ids().last().unwrap();
    wait(&mut w, ada, 2);
    let at = w.world.bodies[w.index(id).unwrap()].structure.position[0] - east * 0.5;
    let why = w.reached(ada, Some(id), at, w.tick as f64).unwrap_err();
    assert!(why.contains("not near a player"), "{why}");
}

#[test]
fn the_ground_behind_a_mound_is_out_of_sight() {
    let (mut w, ada, centre, up, east) = standing();
    let target = w.ground_under(centre + east * 2.0);
    assert!(w.reached(ada, None, target, w.tick as f64).is_ok());
    // A lump of rock between her eye and it.
    let eye = creeer_sim::reach::eye(centre, up);
    let mut edits = Edits::default();
    edits.push(Edit::Fill(
        Brush::Sphere {
            center: eye.lerp(target, 0.5),
            radius: 0.3,
        },
        0,
    ));
    w.apply(&Command::Edit(edits.to_bytes()), None).unwrap();
    // (Fill blends into the ground: the surface there has risen a little.)
    let target = w.ground_under(centre + east * 2.0);
    assert!(
        w.terrain.density(eye.lerp(target, 0.5)) < 0.0,
        "the lump in the way"
    );
    let why = w.reached(ada, None, target, w.tick as f64).unwrap_err();
    assert!(why.contains("behind the ground"), "{why}");
}

#[test]
fn the_sandbox_commands_are_a_creative_servers_alone() {
    let (mut w, ada, centre, up, east) = standing();
    w.creative = false;
    let crate_at = centre + east * 2.0 + up * 0.6;
    let why = w.apply(&Command::Crate(crate_at), Some(ada)).unwrap_err();
    assert!(why.contains("creative"), "{why}");
    assert!(w.apply(&Command::Clear, Some(ada)).is_err());
    // A player's look is theirs anywhere; the catalogue's tools a creative server's alone (in
    // survival a player holds what they made, G1.6b); the host's own commands are unchecked.
    let why = w
        .apply(&Command::Wield { tool: Some(0) }, Some(ada))
        .unwrap_err();
    assert!(why.contains("make one"), "{why}");
    w.apply(&Command::Wield { tool: None }, Some(ada)).unwrap();
    w.apply(&Command::Crate(crate_at), None).unwrap();
    w.creative = true;
    w.apply(&Command::Crate(crate_at + up * 1.5), Some(ada))
        .unwrap();
}
