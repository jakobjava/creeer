//! Impacts that deform: the kinetic energy of a crash must go into plastic work, as it does
//! in reality, not vanish in the time step.

use super::shape::StaticShape;
use super::*;
use crate::structure::section::{Material, STEEL_S235};
use crate::structure::{Section, StructureBuilder};
use creeer_math::det::pow;
use creeer_math::glam::DMat3;

const DT: f64 = 1.0 / 60.0;

/// A 1 t mass on the end of a 2 m steel tube (200 × 5 mm) hits a rigid wall tube-first at
/// `speed`. The tube's front stops at the wall and the mass is braked by the tube's squash
/// load N_p = σ_y·A, so the tube shortens plastically by
///
///   δ = (½·M·v² − N_p²/2k − U_c(N_p)) / N_p
///
/// with M the mass behind the tube (the lump and the rear half of the tube's own mass): at
/// the largest crush the tube (k = EA/L) and the contact still hold elastic energy at the
/// load N_p, which springs back; the contact's (Hertz, `contact/law.rs`) is 2/5·N_p·δ_c.
/// Returns the measured and the expected plastic shortening (m), and the world.
fn crush(speed: f64) -> (f64, f64, World) {
    // Perfectly plastic, as the formula is (real steel: `a_real_steel_crash_*`).
    let steel = Material {
        damping: 0.0,
        ..STEEL_S235.perfectly_plastic()
    };
    let (world, _) = crash(speed, steel);
    let s = &world.bodies[0].structure;
    let e = &s.elements[0];
    let measured = -e.plastic[0];
    let n_p = STEEL_S235.yield_strength * section().area;
    let k = STEEL_S235.youngs_modulus * section().area / LENGTH;
    let behind = LUMP + 0.5 * section().area * LENGTH * STEEL_S235.density;
    let contact = 0.4 * n_p * front_contact(&world).depth(n_p);
    let expected = (0.5 * behind * speed * speed - 0.5 * n_p * n_p / k - contact) / n_p;
    (measured, expected, world)
}

/// The tube: 200 × 5 mm, 2 m long, a 1 t lump behind it.
fn section() -> Section {
    Section::round_tube(0.2, 0.005)
}
const LENGTH: f64 = 2.0;
const LUMP: f64 = 1000.0;

/// The front joint's contact law with the wall.
fn front_contact(world: &World) -> contact::law::Hertz {
    let modulus = world.bodies[0]
        .surface
        .contact_modulus(&world.static_surface);
    contact::law::Hertz::new(modulus, 0.1)
}

/// The lump on the tube hitting the wall at `speed`, the tube of `steel`, for a second;
/// with the plastic work of every step.
fn crash(speed: f64, steel: Material) -> (World, f64) {
    let (section, length, lump) = (section(), LENGTH, LUMP);
    let mut b = StructureBuilder::new();
    let front = b.node(DVec3::ZERO);
    let rear = b.node(DVec3::new(-length, 0.0, 0.0));
    b.mass(rear, lump, DMat3::from_diagonal(DVec3::splat(50.0)));
    // Undamped: steel's stiffness-proportional damping (calibrated at 2 Hz) would also take
    // the kHz rattle of the front joint against the wall, 5 % of a 5 m/s crash with Hertz's
    // contact, which the formula leaves out (real steel rings far longer; ADR-0005).
    b.beam(front, rear, section, steel);
    let mut structure = b.build();
    structure.gravity = DVec3::ZERO;
    // The tube squashes, as the formulas say; buckling has its own tests (`buckling_tests.rs`).
    structure.splits = false;
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
    let start = world.energy();
    for _ in 0..60 {
        world.step();
    }
    let lost = start - world.energy();
    (world, lost)
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

/// The tube's plastic shortening (m) in real steel, from the same two masses in continuous
/// time: the lump and the rear half of the tube behind, the front half on the wall (Hertz),
/// the elastic tube between them, flowing plastically by Cowper–Symonds as an overstress:
/// ε̇ = D·(N/(A·σ(ε)) − 1 + (ε̇₀/D)^{1/q})^q once N passes A·σ(ε) (`Material::flow_stress`,
/// `RateSensitivity`). RK4 in steps of 1 µs, far below the flow's relaxation (~90 µs).
fn continuous(speed: f64, world: &World) -> f64 {
    let steel = STEEL_S235;
    let (area, r) = (section().area, steel.rate);
    let half = 0.5 * area * LENGTH * steel.density;
    let (behind, front) = (LUMP + half, half);
    let ea = steel.youngs_modulus * area;
    let wall = front_contact(world);
    let offset = pow(r.reference / r.d, 1.0 / r.q);
    // State: the rear and the front's travel past touching, their speeds, plastic strain.
    let derivative = |[xr, xf, vr, vf, strain]: [f64; 5]| -> [f64; 5] {
        let force = ea * ((xr - xf) / LENGTH - strain).max(0.0);
        let static_force = area * steel.flow_stress(strain);
        let rate = if force > static_force {
            r.d * pow(force / static_force - 1.0 + offset, r.q)
        } else {
            0.0
        };
        [
            vr,
            vf,
            -force / behind,
            (force - wall.force(xf)) / front,
            rate,
        ]
    };
    let mut y = [0.0, 0.0, speed, speed, 0.0];
    let dt = 1e-6;
    for _ in 0..200_000 {
        let k1 = derivative(y);
        let k2 = derivative(std::array::from_fn(|i| y[i] + 0.5 * dt * k1[i]));
        let k3 = derivative(std::array::from_fn(|i| y[i] + 0.5 * dt * k2[i]));
        let k4 = derivative(std::array::from_fn(|i| y[i] + dt * k3[i]));
        y = std::array::from_fn(|i| y[i] + dt / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]));
    }
    y[4] * LENGTH
}

/// In real steel the tube hardens as it crushes and resists faster crushing more (at
/// 20 m/s it is strained at 10 s⁻¹, where mild steel flows 1.7 times as hard): it crushes
/// as the same two masses do in continuous time, within 2 %, and
/// far less than perfectly plastic steel would (at 20 m/s: 0.16 m against 0.28 m).
#[test]
fn a_real_steel_crash_crushes_as_hardening_and_strain_rate_say() {
    for speed in [5.0, 10.0, 20.0] {
        let steel = Material {
            damping: 0.0,
            ..STEEL_S235
        };
        let (world, _) = crash(speed, steel);
        let measured = -world.bodies[0].structure.elements[0].plastic[0];
        let expected = continuous(speed, &world);
        assert!(
            (measured / expected - 1.0).abs() < 0.02,
            "{speed} m/s: crushed {measured:.4} m, expected {expected:.4} m"
        );
    }
}

/// A 10 t block dropped 0.5 m onto the sandbox's 40 m bridge (267 tubes of 160 × 8 mm on two
/// piers) collapses it plastically by about 0.37 m over half a second. The members flow at
/// 0.4 /s at most, slowly enough for 60 Hz steps once the impact itself is over
/// (`FAST_FLOW`): the sag and the plastic strain are those of 0.5 ms substeps throughout
/// (0.374 m and 0.849 summed over the members, measured so), within 2 %.
#[test]
fn a_bridge_collapsing_under_a_block_sags_as_impact_substeps_have_it() {
    use crate::structure::shapes::{Base, space_frame};
    let mut world = World::new(DT);
    world.statics.push(StaticShape::Plane {
        normal: DVec3::Y,
        offset: 0.0,
    });
    let tube = Section::round_tube(0.16, 0.008);
    let mut deck = space_frame([20, 1, 2], 2.0, tube, STEEL_S235, Base::Free);
    deck.translate(DVec3::new(-20.0, 4.0 + tube.extent[0], -2.0));
    world.add(Body::structure(deck, DT));
    for x in [-20.5, 20.5] {
        world.statics.push(StaticShape::Box {
            center: DVec3::new(x, 2.0, 0.0),
            rotation: creeer_math::glam::DQuat::IDENTITY,
            half: DVec3::new(1.0, 2.0, 3.0),
        });
    }
    let deck_top = 6.0 + 2.0 * tube.extent[0];
    let block = Shape::Box {
        half: DVec3::new(2.2, 0.5, 2.2),
    };
    world.add(Body::rigid(
        block,
        10_000.0,
        DVec3::new(0.0, deck_top + 1.0, 0.0),
        DT,
    ));
    for _ in 0..300 {
        world.step();
    }
    let sag = deck_top + 0.5 - world.bodies[1].structure.position[0].y;
    let strain: f64 = world.bodies[0]
        .structure
        .elements
        .iter()
        .map(|e| e.plastic_strain)
        .sum();
    assert!((sag / 0.374 - 1.0).abs() < 0.02, "sagged {sag:.4} m");
    assert!(
        (strain / 0.849 - 1.0).abs() < 0.02,
        "plastic strain {strain:.4}"
    );
}
