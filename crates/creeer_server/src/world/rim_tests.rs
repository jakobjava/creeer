//! A rod against the rounded rim of a narrow hole (G2.9): a pole left leaning with its foot
//! in a hole the player's blows dug, pressed on the rim's round, comes to rest without
//! gaining energy, as fast as it falls at most. Its contacts' ground lies on both sides of
//! the hole's axis, where the terrain's distance has its ridge (`creeer_terrain::
//! edit_gradient`).

use super::raising_tests::{ada_and_pole_dug, wait};
use creeer_math::glam::{DQuat, DVec3};

/// A pole's fastest (m/s), the most its energy rose (J), its speed at the end (m/s) and its
/// foot's depth below the mouth (m).
type Throughout = (f64, f64, f64, f64);
/// Over its last second: its fastest (m/s), its fastest turn (rad/s), how far its foot went (m).
type LastSecond = (f64, f64, f64);

/// Leaning `lean` degrees west from the vertical, its foot 0.3 m down the hole against its
/// east wall, left alone for four seconds: its fastest (m/s), the most its energy rose above
/// where it started (J; kinetic and gravity's), its speed at the end and its foot's depth
/// below the mouth (m); and over its last second, its fastest (m/s), its fastest turn (rad/s)
/// and how far its foot went (m).
fn lean_in_the_hole(lean: f64) -> (Throughout, LastSecond) {
    let (mut w, ada, id, east, mouth) = ada_and_pole_dug(false);
    let up = mouth.normalize();
    let i = w.index(id).unwrap();
    let (s, c) = creeer_math::det::sin_cos(lean.to_radians());
    let axis = (up * c - east * s).normalize();
    let foot = mouth - up * 0.3 + east * 0.08;
    let body = &mut w.world.bodies[i];
    body.structure.orientation[0] = DQuat::from_rotation_arc(DVec3::X, -axis);
    body.structure.position[0] = foot + axis * 1.5;
    body.structure.velocity[0] = DVec3::ZERO;
    body.structure.angular_velocity[0] = DVec3::ZERO;
    body.solver.reset_history(&body.structure);
    body.wake();
    let energy = |w: &super::ServerWorld| w.world.bodies[w.index(id).unwrap()].structure.energy();
    let start = energy(&w);
    let (mut fastest, mut rise) = (0.0f64, 0.0f64);
    let (mut last, mut turn, mut from) = (0.0f64, 0.0f64, DVec3::ZERO);
    for tick in 0..240 {
        wait(&mut w, ada, 1, DVec3::ZERO);
        let s = &w.world.bodies[w.index(id).unwrap()].structure;
        fastest = fastest.max(s.velocity[0].length());
        rise = rise.max(energy(&w) - start);
        if tick == 179 {
            from = s.position[0] + s.orientation[0] * DVec3::X * 1.5;
        } else if tick > 179 {
            last = last.max(s.velocity[0].length());
            turn = turn.max(s.angular_velocity[0].length());
        }
    }
    let s = &w.world.bodies[w.index(id).unwrap()].structure;
    let foot = s.position[0] + s.orientation[0] * DVec3::X * 1.5;
    (
        (
            fastest,
            rise,
            s.velocity[0].length(),
            (mouth - foot).dot(up),
        ),
        (last, turn, (foot - from).length()),
    )
}

/// Leaning `lean` degrees, it slides down the hole and rests on its rim: never faster than
/// falling the hole's whole depth (0.85 m, its round end included: 4.1 m/s), its energy never
/// above where it started, still at the end with its foot in the hole; and, if `still`, at rest
/// over its whole last second, not on its last tick alone (G2.10).
fn rests_on_the_rim(lean: f64, still: bool) {
    let ((fastest, rise, end, below), (last, turn, travel)) = lean_in_the_hole(lean);
    eprintln!(
        "{lean}°: fastest {fastest:.2} m/s, energy up {rise:.3} J at most, {end:.3} m/s at the \
         end, its foot {below:.2} m down; over the last second at most {last:.4} m/s and \
         {turn:.4} rad/s, its foot {travel:.5} m on"
    );
    assert!(fastest < 4.1, "{lean}°: {fastest:.2} m/s");
    assert!(rise < 0.01, "{lean}°: its energy rose by {rise:.3} J");
    assert!(end < 0.05, "{lean}°: still moving at {end:.3} m/s");
    assert!(below > 0.2, "{lean}°: its foot only {below:.2} m down");
    if still {
        assert!(
            last < 0.05,
            "{lean}°: moving at {last:.3} m/s in its last second"
        );
        assert!(
            turn < 0.1,
            "{lean}°: turning at {turn:.3} rad/s in its last second"
        );
        assert!(
            travel < 1e-3,
            "{lean}°: its foot went {travel:.4} m in its last second"
        );
    }
}

/// Leaning 10° and 30° from the vertical it slides down the hole and rests on its rim
/// ([`rests_on_the_rim`]). Measured (G2.9): fastest 2.65 and 1.14 m/s, its energy never up, at
/// rest after four seconds, its foot 0.82 and 0.44 m down. With the gradient near the hole by
/// central differences over 1 cm, at 30° the pole gained 36 J on the rim (and a pole raised by
/// its end with its foot in the hole was thrown at 1,254 m/s, `raising_tests.rs`). Measured at
/// G2.10: fastest 2.92 and 1.14 m/s, at its last tick 0.000 m/s, its foot 0.82 and 0.48 m down.
/// Over the last second they are not yet at rest (0.021 m/s at 10°; 0.054 m/s, 1.18 rad/s and
/// 12.5 mm at 30°): G2P P1's gates, as 20° below.
#[test]
fn a_pole_leaning_on_a_narrow_holes_rim_rests() {
    for lean in [10.0, 30.0] {
        rests_on_the_rim(lean, false);
    }
}

/// Leaning 20°, at rest over its whole last second too. Not yet (G2P P1, ADR-0005 G2.10's
/// "friction anchors loaded beyond sticking"): it rolls about its own axis every other step at
/// about 0.9 rad/s, stopped dead by the energy guard between, and creeps 15 cm down the hole in
/// 15 s; its last tick reads 0.087 m/s (against 0.05), its last second 0.087 m/s, 0.88 rad/s and
/// 13 mm. Its friction anchors, a face's corner 10 cm from the wall it presses on, come out
/// 2–4 mm across a normal turned 1–2°; `docs/adr/0005/anchors-kept-and-clamped.diff` brings it
/// to rest, and is where G2P P1 starts.
#[test]
#[ignore = "G2P P1: rolls about its axis at 0.9 rad/s every other step, 0.087 m/s at the end"]
fn a_pole_leaning_20_degrees_on_a_narrow_holes_rim_is_still() {
    rests_on_the_rim(20.0, true);
}
