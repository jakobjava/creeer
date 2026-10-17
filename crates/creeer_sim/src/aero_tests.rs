//! Bodies in the air: a falling body against v_t·tanh(g·t/v_t), a sphere at its terminal
//! velocity on the drag curve, a dart turning into its flight, a crate under a canopy coming
//! down at the canopy's terminal velocity with the air's work booked.

use crate::aero::{Airframe, Datum, Sky, push, settle};
use creeer_fluids::aero::{Section, Shape, Surface};
use creeer_fluids::atmosphere::Atmosphere;
use creeer_materials::species::SpeciesTable;
use creeer_math::det;
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::structure::STANDARD_GRAVITY;
use creeer_physics::world::shape::Shape as Collider;
use creeer_physics::world::{Body, World};
use creeer_terrain::AtmosphereData;

const DT: f64 = 1.0 / 60.0;
const R0: f64 = 6_356_766.0;
const G: f64 = 9.806_65;

/// Air at sea level (below the datum the atmosphere holds its sea-level state: the bodies
/// fall through still, uniform air).
fn sky() -> Sky {
    let data = AtmosphereData {
        composition: vec![
            ("N2".into(), 0.78084),
            ("O2".into(), 0.20946),
            ("Ar".into(), 0.00934),
            ("CO2".into(), 0.00036),
        ],
        surface_pressure: 101_325.0,
        surface_temperature: 288.15,
        layers: vec![(11_000.0, -6.5e-3), (20_000.0, 0.0)],
    };
    let atmosphere = Atmosphere::new(&data, R0, G * R0 * R0, &SpeciesTable::reference()).unwrap();
    Sky {
        atmosphere,
        datum: Datum::Flat,
        wind: DVec3::ZERO,
    }
}

fn step(sky: &Sky, world: &mut World, frames: &mut [Airframe]) {
    push(sky, None, world, frames);
    world.step();
    settle(world, frames);
}

/// A 5 kg body of fixed drag (C_d·A = 0.1 m²) dropped from rest: v = v_t·tanh(g·t/v_t),
/// v_t = √(2mg/(ρ·C_d·A)), within 0.2 % over 20 s.
#[test]
fn a_falling_body_reaches_its_terminal_velocity() {
    let sky = sky();
    let rho = sky.air(DVec3::ZERO).density;
    let mut world = World::new(DT);
    let body = world.add(Body::rigid(
        Collider::Sphere { radius: 0.2 },
        5.0,
        DVec3::new(0.0, -1.0, 0.0),
        DT,
    ));
    let mut frames = vec![Airframe::new(
        body,
        0,
        Surface {
            shape: Shape::Bluff {
                area: 0.1,
                drag: 1.0,
            },
            offset: DVec3::ZERO,
        },
    )];
    let vt = (2.0 * 5.0 * G / (rho * 0.1)).sqrt();
    let mut t = 0.0;
    for _ in 0..1200 {
        step(&sky, &mut world, &mut frames);
        t += DT;
        let v = -world.bodies[body].structure.velocity[0].y;
        let exact = vt * det::tanh(G * t / vt);
        assert!(
            (v - exact).abs() < 2e-3 * vt,
            "t {t}: {v} m/s, exact {exact}"
        );
    }
}

/// A 7 cm steel ball (1.41 kg) falls until the drag curve's force bears its weight:
/// ½ρ·C_d(Re)·A·v² = mg, at about 100 m/s (Re 4.7·10⁵, just before the crisis).
#[test]
fn a_sphere_falls_at_its_drag_curves_terminal_velocity() {
    let sky = sky();
    let air = sky.air(DVec3::ZERO);
    let (d, m) = (
        0.07,
        7850.0 * std::f64::consts::PI / 6.0 * 0.07 * 0.07 * 0.07,
    );
    let mut world = World::new(DT);
    let body = world.add(Body::rigid(
        Collider::Sphere { radius: 0.5 * d },
        m,
        DVec3::new(0.0, -1.0, 0.0),
        DT,
    ));
    let surface = Surface {
        shape: Shape::Sphere { diameter: d },
        offset: DVec3::ZERO,
    };
    let mut frames = vec![Airframe::new(body, 0, surface)];
    for _ in 0..60 * 120 {
        step(&sky, &mut world, &mut frames);
    }
    let v = world.bodies[body].structure.velocity[0];
    let drag = surface.force(DMat3::IDENTITY, v, &air);
    assert!(
        (drag.y / (m * G) - 1.0).abs() < 1e-3,
        "drag {} N, weight {} N at {} m/s",
        drag.y,
        m * G,
        v.y
    );
}

/// A dart: a 2 kg body with a 0.04 m² fin cross 1 m behind it, launched at 50 m/s pointing
/// 10° off its flight: the fins turn it into the wind, and it stays there.
#[test]
fn a_dart_turns_into_its_flight() {
    let sky = sky();
    let mut world = World::new(DT);
    let body = world.add(Body::rigid(
        Collider::Sphere { radius: 0.1 },
        2.0,
        DVec3::new(0.0, -1.0, 0.0),
        DT,
    ));
    world.bodies[body].structure.gravity = DVec3::ZERO;
    let fin = |normal: DVec3| Surface {
        shape: Shape::Panel {
            area: 0.04,
            aspect: 2.0,
            efficiency: 0.9,
            section: Section::PLATE,
            normal,
            chord: DVec3::X,
        },
        offset: DVec3::new(-1.0, 0.0, 0.0),
    };
    let mut frames = vec![
        Airframe::new(body, 0, fin(DVec3::Y)),
        Airframe::new(body, 0, fin(DVec3::Z)),
    ];
    let off = 10f64.to_radians();
    let (sin, cos) = det::sin_cos(off);
    world.bodies[body].structure.velocity[0] = DVec3::new(cos, -sin, 0.0) * 50.0;
    for _ in 0..600 {
        step(&sky, &mut world, &mut frames);
    }
    let s = &world.bodies[body].structure;
    let nose = DMat3::from_quat(s.orientation[0]) * DVec3::X;
    let angle = det::acos(nose.dot(s.velocity[0].normalize()).min(1.0));
    assert!(
        angle < 0.5f64.to_radians(),
        "{} degrees off its flight",
        angle.to_degrees()
    );
}

/// A 50 kg crate (C_d 1.05, 0.36 m²) under a 20 m² canopy (C_d 1.3) 4 m above it, dropped at
/// 30 m/s: it comes down at √(2mg/ρΣC_dA) and hangs under the canopy; the kinetic and
/// potential energy it lost is the work the air did, to 0.1 %.
#[test]
fn a_crate_comes_down_under_its_canopy() {
    let sky = sky();
    let rho = sky.air(DVec3::ZERO).density;
    let mut world = World::new(DT);
    let body = world.add(Body::rigid(
        Collider::Box {
            half: DVec3::splat(0.3),
        },
        50.0,
        DVec3::new(0.0, -1.0, 0.0),
        DT,
    ));
    {
        let s = &mut world.bodies[body].structure;
        s.velocity[0] = DVec3::new(5.0, -30.0, 0.0);
        s.orientation[0] = creeer_physics::world::rotation(DVec3::Z, 0.3);
    }
    let crate_drag = Surface {
        shape: Shape::Bluff {
            area: 0.36,
            drag: 1.05,
        },
        offset: DVec3::ZERO,
    };
    let canopy = Surface {
        shape: Shape::Bluff {
            area: 20.0,
            drag: 1.3,
        },
        offset: DVec3::new(0.0, 4.0, 0.0),
    };
    let mut frames = vec![
        Airframe::new(body, 0, crate_drag),
        Airframe::new(body, 0, canopy),
    ];
    let energy = |w: &World| {
        let s = &w.bodies[body].structure;
        let (kinetic, _) = s.node_energy();
        kinetic - 50.0 * STANDARD_GRAVITY.dot(s.position[0])
    };
    let before = energy(&world);
    for _ in 0..60 * 60 {
        step(&sky, &mut world, &mut frames);
    }
    let s = &world.bodies[body].structure;
    let vt = (2.0 * 50.0 * G / (rho * (0.36 * 1.05 + 20.0 * 1.3))).sqrt();
    assert!(
        (-s.velocity[0].y / vt - 1.0).abs() < 1e-3,
        "{} m/s, {vt}",
        s.velocity[0].y
    );
    let up = DMat3::from_quat(s.orientation[0]) * DVec3::Y;
    assert!(up.y > 0.9999, "hangs under it: {up}");
    let work: f64 = frames.iter().map(|f| f.work).sum();
    let lost = energy(&world) - before;
    assert!(
        (lost / work - 1.0).abs() < 1e-3,
        "lost {lost} J, the air did {work} J"
    );
}
