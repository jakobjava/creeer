//! Impacts that deform: the kinetic energy of a crash must go into plastic work, as it does
//! in reality, not vanish in the time step.

use super::shape::StaticShape;
use super::*;
use crate::structure::section::STEEL_S235;
use crate::structure::{Section, StructureBuilder};
use creeer_math::glam::DMat3;

const DT: f64 = 1.0 / 60.0;

/// A 1 t mass on the end of a 2 m steel tube (200 × 5 mm) hits a rigid wall tube-first at
/// `speed`. The tube's front stops at the wall and the mass is braked by the tube's squash
/// load N_p = σ_y·A, so the tube shortens plastically by
///
///   δ = (½·M·v² − N_p²/2·(1/k + 1/k_c)) / N_p
///
/// with M the mass behind the tube (the lump and the rear half of the tube's own mass): at
/// the largest crush the tube (k = EA/L) and the contact (k_c) still hold elastic energy
/// at the load N_p, which springs back.
/// Returns the measured and the expected plastic shortening (m), and the world.
fn crush(speed: f64) -> (f64, f64, World) {
    let section = Section::round_tube(0.2, 0.005);
    let length = 2.0;
    let lump = 1000.0;
    let mut b = StructureBuilder::new();
    let front = b.node(DVec3::ZERO);
    let rear = b.node(DVec3::new(-length, 0.0, 0.0));
    b.mass(rear, lump, DMat3::from_diagonal(DVec3::splat(50.0)));
    b.beam(front, rear, section, STEEL_S235);
    let mut structure = b.build();
    structure.gravity = DVec3::ZERO;
    for v in &mut structure.velocity {
        *v = DVec3::X * speed;
    }
    let mut world = World::new(DT);
    // The front joint is a ball of radius 0.1 m; the wall's face is 5 cm in front of it.
    world.statics.push(StaticShape::Box {
        center: DVec3::new(1.15, 0.0, 0.0),
        rotation: DQuat::IDENTITY,
        half: DVec3::splat(1.0),
    });
    world.add(Body::structure(structure, DT));
    for _ in 0..60 {
        world.step();
    }
    let s = &world.bodies[0].structure;
    let e = &s.elements[0];
    let measured = -e.plastic[0];
    let n_p = STEEL_S235.yield_strength * section.area;
    let k = STEEL_S235.youngs_modulus * section.area / length;
    let behind = lump + 0.5 * section.area * length * STEEL_S235.density;
    let springs = 1.0 / k + 1.0 / world.contact_stiffness;
    let expected = (0.5 * behind * speed * speed - 0.5 * n_p * n_p * springs) / n_p;
    (measured, expected, world)
}

/// The crush matches the plastic work it takes to stop the mass, within 2 %, from a light
/// knock (where the elastic energy is a quarter of the total) to a hard crash (14 % strain).
#[test]
fn a_crash_crushes_by_the_plastic_work() {
    for speed in [5.0, 10.0, 20.0] {
        let (measured, expected, _) = crush(speed);
        assert!(
            (measured / expected - 1.0).abs() < 0.02,
            "{speed} m/s: crushed {measured:.4} m, expected {expected:.4} m"
        );
    }
}

/// At 30 m/s the crush would be 0.64 m, a strain of 32 %: the tube tears once its plastic
/// strain passes steel's elongation at break (26 %, at 0.52 m), and the mass flies on.
#[test]
fn a_crash_beyond_the_elongation_at_break_tears_the_tube() {
    let (measured, expected, world) = crush(30.0);
    let e = &world.bodies[0].structure.elements[0];
    assert!(
        expected > STEEL_S235.elongation * 2.0,
        "the setup must overload"
    );
    assert!(e.broken, "the tube survived a crush of {expected:.2} m");
    let at_break = STEEL_S235.elongation * 2.0;
    assert!(
        (measured / at_break - 1.0).abs() < 0.02,
        "tore after {measured:.3} m, the elongation allows {at_break:.3} m"
    );
}
