//! A rod against the rounded rim of a narrow hole (G2.9): a pole left leaning with its foot
//! in a hole the player's blows dug, pressed on the rim's round, comes to rest without
//! gaining energy, as fast as it falls at most. Its contacts' ground lies on both sides of
//! the hole's axis, where the terrain's distance has its ridge (`creeer_terrain::
//! edit_gradient`).

use super::raising_tests::{ada_and_pole, wait};
use creeer_math::glam::{DQuat, DVec3};

/// Leaning `lean` degrees west from the vertical, its foot 0.3 m down the hole against its
/// east wall, left alone for four seconds: its fastest (m/s), the most its energy rose above
/// where it started (J; kinetic and gravity's), its speed at the end and its foot's depth
/// below the mouth (m).
fn lean_in_the_hole(lean: f64) -> (f64, f64, f64, f64) {
    let (mut w, ada, id, east, mouth) = ada_and_pole();
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
    for _ in 0..240 {
        wait(&mut w, ada, 1, DVec3::ZERO);
        let s = &w.world.bodies[w.index(id).unwrap()].structure;
        fastest = fastest.max(s.velocity[0].length());
        rise = rise.max(energy(&w) - start);
    }
    let s = &w.world.bodies[w.index(id).unwrap()].structure;
    let foot = s.position[0] + s.orientation[0] * DVec3::X * 1.5;
    (
        fastest,
        rise,
        s.velocity[0].length(),
        (mouth - foot).dot(up),
    )
}

/// Leaning 10°, 20° and 30° from the vertical it slides down the hole and rests on its rim:
/// never faster than falling the hole's whole depth (0.85 m, its round end included: 4.1 m/s),
/// its energy never above where it started. Measured (G2.9): fastest 2.65, 2.11 and 1.14 m/s,
/// its energy never up, at rest after four seconds, its foot 0.82, 0.65 and 0.44 m down. With
/// the gradient near the hole by central differences over 1 cm, at 30° the pole gained 36 J
/// on the rim (and a pole raised by its end with its foot in the hole was thrown at
/// 1,254 m/s, `raising_tests.rs`).
#[test]
fn a_pole_leaning_on_a_narrow_holes_rim_rests() {
    for lean in [10.0, 20.0, 30.0] {
        let (fastest, rise, end, below) = lean_in_the_hole(lean);
        eprintln!(
            "{lean}°: fastest {fastest:.2} m/s, energy up {rise:.3} J at most, {end:.3} m/s at \
             the end, its foot {below:.2} m down"
        );
        assert!(fastest < 4.1, "{lean}°: {fastest:.2} m/s");
        assert!(rise < 0.01, "{lean}°: its energy rose by {rise:.3} J");
        assert!(end < 0.05, "{lean}°: still moving at {end:.3} m/s");
        assert!(below > 0.2, "{lean}°: its foot only {below:.2} m down");
    }
}
