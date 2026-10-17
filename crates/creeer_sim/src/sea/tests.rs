//! Floating against its classics: the spherical cap, a box cut at any angle, Archimedes'
//! draft, the wall-sided righting arm GZ = sin θ·(GM + ½·BM·tan² θ), a wide box righting
//! itself and a tall one capsizing, the hydrostatic pressure, the sea level of a turning
//! planet level to its gravity, the books of a ball dropped in.

use super::volume::{Waterline, submerged};
use super::{Floater, Level, Sea, push, settle};
use creeer_math::det;
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::gravity::PlanetGravity;
use creeer_physics::world::shape::Shape;
use creeer_physics::world::{Body, World, rotation};
use creeer_terrain::OceanData;
use creeer_terrain::datum::Datum;
use std::f64::consts::PI;

const DT: f64 = 1.0 / 60.0;
const G: f64 = 9.806_65;
const WATER: OceanData = OceanData {
    density: 1025.97,
    viscosity: 1.22e-3,
    sound: 1506.7,
    temperature: 288.15,
};
const SEA: Sea = Sea {
    level: Level::Flat(0.0),
    water: WATER,
    surface_pressure: 101_325.0,
};

fn level(normal: DVec3, level: f64) -> Waterline {
    Waterline {
        normal: normal.normalize(),
        level,
    }
}

/// A sphere half under: ⅔πr³, its centre of buoyancy 3r/8 down; wholly under, all of it.
/// A box cut through its centre at any slant: half of it, the halves' centroids opposite;
/// a hull of the same box the same as the box.
#[test]
fn submerged_volumes_are_exact() {
    let ball = Shape::Sphere { radius: 2.0 };
    let (v, c) = submerged(&ball, &level(DVec3::Y, 0.0));
    assert!(
        (v - 2.0 / 3.0 * PI * 8.0).abs() < 1e-12 && (c.y + 0.75).abs() < 1e-12,
        "{v} {c}"
    );
    assert!((submerged(&ball, &level(DVec3::Y, 5.0)).0 - 4.0 / 3.0 * PI * 8.0).abs() < 1e-12);
    let half = DVec3::new(1.0, 0.4, 0.7);
    let cube = Shape::Box { half };
    let hull = Shape::Hull(std::sync::Arc::new(
        creeer_physics::world::hull::Hull::cuboid(half),
    ));
    let whole = 8.0 * half.x * half.y * half.z;
    for normal in [
        DVec3::new(0.3, 1.0, -0.2),
        DVec3::new(-1.0, 0.1, 0.4),
        DVec3::new(0.0, 0.0, 1.0),
    ] {
        let (v, c) = submerged(&cube, &level(normal, 0.0));
        let (u, d) = submerged(&cube, &level(-normal, 0.0));
        assert!((v - 0.5 * whole).abs() < 1e-12 && (u - 0.5 * whole).abs() < 1e-12);
        assert!((c + d).length() < 1e-12, "{c} {d}");
        for at in [-0.3, 0.2] {
            let (a, ca) = submerged(&cube, &level(normal, at));
            let (b, cb) = submerged(&hull, &level(normal, at));
            assert!((a - b).abs() < 1e-12 && (ca - cb).length() < 1e-12);
        }
    }
}

/// A box of density ρ_b floats at the draft ρ_b/ρ_w·h (Archimedes).
#[test]
fn a_box_floats_at_its_draft() {
    let half = DVec3::new(0.5, 0.2, 0.5);
    let mass = 600.0 * 8.0 * half.x * half.y * half.z;
    let mut world = World::new(DT);
    let body = world.add(Body::rigid(
        Shape::Box { half },
        mass,
        DVec3::new(0.0, 0.3, 0.0),
        DT,
    ));
    let mut floaters = vec![Floater::new(body, 0, Shape::Box { half }, 1.0)];
    for _ in 0..60 * 30 {
        push(&SEA, &mut world, &mut floaters);
        world.step();
        settle(&mut world, &mut floaters, DT);
    }
    let draft = 600.0 / WATER.density * 0.4;
    let bottom = world.bodies[body].structure.position[0].y - 0.2;
    assert!(
        (bottom + draft).abs() < 1e-4,
        "bottom at {bottom}, draft {draft}"
    );
}

/// The righting moment of a box floating heeled by 10°: ρ·g·V·GZ with the wall-sided
/// GZ = sin θ·(GM + ½·BM·tan² θ), BM = b²/12T, GM = KB + BM − KG (exact while neither its
/// deck nor its bottom's edge crosses the water).
#[test]
fn a_heeled_box_rights_by_the_wall_sided_formula() {
    let (b, height, density) = (2.0, 0.5, 500.0);
    let half = DVec3::new(0.5 * b, 0.5 * height, 1.0);
    let draft = density / WATER.density * height;
    let theta = 10f64.to_radians();
    let mut world = World::new(DT);
    // Heeled about the waterplane's centre (so the displacement stays the upright one's).
    let heel = rotation(DVec3::Z, theta);
    let centre = heel * DVec3::new(0.0, 0.5 * height - draft, 0.0);
    let body = world.add(Body::rigid(
        Shape::Box { half },
        density * 8.0 * half.x * half.y * half.z,
        centre,
        DT,
    ));
    world.bodies[body].structure.orientation[0] = heel;
    let mut floaters = vec![Floater::new(body, 0, Shape::Box { half }, 0.0)];
    push(&SEA, &mut world, &mut floaters);
    let f = &floaters[0];
    let volume = draft * b * 2.0;
    assert!(
        (f.volume / volume - 1.0).abs() < 1e-12,
        "{} {volume}",
        f.volume
    );
    let bm = b * b / (12.0 * draft);
    let gm = 0.5 * draft + bm - 0.5 * height;
    let (sin, cos) = det::sin_cos(theta);
    let gz = sin * (gm + 0.5 * bm * (sin / cos) * (sin / cos));
    let righting = WATER.density * G * volume * gz;
    assert!(
        (-f.torque.z / righting - 1.0).abs() < 1e-3,
        "{} N·m, wall-sided {righting}",
        -f.torque.z
    );
}

/// Let go heeled, a wide flat box (GM > 0) rolls back about upright, ever less (lightly
/// damped: its waves are not yet modelled, its corners' eddies by drag); a tall narrow one
/// (GM < 0) capsizes onto its side.
#[test]
fn wide_boxes_right_themselves_and_tall_ones_capsize() {
    for (half, tilt, upright) in [
        (DVec3::new(1.0, 0.25, 1.0), 20.0, true),
        (DVec3::new(0.25, 1.0, 0.25), 5.0, false),
    ] {
        let mut world = World::new(DT);
        let draft = 0.5 * 2.0 * half.y;
        let body = world.add(Body::rigid(
            Shape::Box { half },
            500.0 * 8.0 * half.x * half.y * half.z,
            DVec3::new(0.0, half.y - draft, 0.0),
            DT,
        ));
        world.bodies[body].structure.orientation[0] = rotation(DVec3::Z, f64::to_radians(tilt));
        let mut floaters = vec![Floater::new(body, 0, Shape::Box { half }, 1.0)];
        // The largest heel in each ten seconds.
        let mut heel = [0.0f64; 4];
        for k in 0..60 * 40 {
            push(&SEA, &mut world, &mut floaters);
            world.step();
            settle(&mut world, &mut floaters, DT);
            let up = DMat3::from_quat(world.bodies[body].structure.orientation[0]) * DVec3::Y;
            heel[k / 600] = heel[k / 600].max(det::acos(up.y.min(1.0)).to_degrees());
        }
        if upright {
            assert!(
                heel.windows(2).all(|w| w[1] < w[0]) && heel[3] < 5.0,
                "the wide box heels {heel:?}"
            );
        } else {
            // On its side, rocking about it.
            let up = DMat3::from_quat(world.bodies[body].structure.orientation[0]) * DVec3::Y;
            let now = det::acos(up.y.min(1.0)).to_degrees();
            assert!(
                (now - 90.0).abs() < 10.0 && heel[3] < 100.0,
                "the tall box heels {heel:?}, now {now}"
            );
        }
    }
}

/// Under the sea the pressure is p₀ + ρ·g·depth; on a turning planet the sea's surface is
/// square to the gravity there, centrifugal force and all.
#[test]
fn the_sea_is_hydrostatic_and_level() {
    let p = SEA.medium(DVec3::new(3.0, -10.0, 1.0), G).p;
    assert!((p - (101_325.0 + WATER.density * G * 10.0)).abs() < 1e-6);
    let (radius, mu, omega) = (500_000.0, 9.81 * 25e10, std::f64::consts::TAU / 21_600.0);
    let sea = Sea {
        level: Level::Planet(Datum {
            polar_radius: radius,
            mu,
            omega,
        }),
        ..SEA
    };
    let gravity = PlanetGravity::planet(mu, DVec3::Y * omega);
    let at = DVec3::new(1.0, 1.0, 0.3).normalize() * (radius + 1000.0);
    let (surface, up) = sea.surface(at);
    let down = gravity.acceleration(surface).normalize();
    assert!((up + down).length() < 1e-6, "up {up}, gravity {down}");
    assert!((sea.depth(surface)).abs() < 1e-3);
}

/// A ball of 300 kg/m³ dropped into the sea from 1 m: it bobs and settles at its draft, and
/// what it lost of its kinetic and potential energy is the work the water did on it.
#[test]
fn a_ball_dropped_in_keeps_the_books() {
    let radius = 0.3;
    let mass = 300.0 * 4.0 / 3.0 * PI * radius * radius * radius;
    let mut world = World::new(DT);
    let body = world.add(Body::rigid(
        Shape::Sphere { radius },
        mass,
        DVec3::new(0.0, 1.0, 0.0),
        DT,
    ));
    let mut floaters = vec![Floater::new(body, 0, Shape::Sphere { radius }, 0.5)];
    let energy = |w: &World| {
        let s = &w.bodies[body].structure;
        s.node_energy().0 + mass * G * s.position[0].y
    };
    let before = energy(&world);
    for _ in 0..60 * 20 {
        push(&SEA, &mut world, &mut floaters);
        world.step();
        settle(&mut world, &mut floaters, DT);
    }
    let lost = energy(&world) - before;
    let work = floaters[0].work;
    assert!(
        (lost / work - 1.0).abs() < 0.01,
        "lost {lost} J, the water did {work} J"
    );
    assert!(world.bodies[body].structure.velocity[0].length() < 1e-3);
}

/// A 1 m box hull of 150 kg floats high; holed in its floor (10 cm across) it floods, the water
/// inside rising to the sea's level outside as Torricelli lets it, and it settles ever lower:
/// with 0.9 m³ inside it would hold 1073 kg of water, more than it can carry, and it sinks.
#[test]
fn a_holed_hull_floods_and_sinks() {
    use super::flood::Flood;
    let half = DVec3::splat(0.5);
    let mut world = World::new(DT);
    let body = world.add(Body::rigid(
        Shape::Box { half },
        150.0,
        DVec3::new(0.0, 0.35, 0.0),
        DT,
    ));
    let mut floaters = vec![Floater::new(body, 0, Shape::Box { half }, 1.0)];
    let hole = 0.25 * PI * 0.1 * 0.1;
    let mut flood = Flood::new(body, 0, DVec3::new(0.0, -0.45, 0.0), hole, 0.95, 0.9);
    let mut floating = 0.0;
    for k in 0..60 * 600 {
        flood.push(&SEA, &mut world, DT);
        push(&SEA, &mut world, &mut floaters);
        world.step();
        settle(&mut world, &mut floaters, DT);
        flood.settle(&mut world);
        if k == 60 * 5 {
            floating = world.bodies[body].structure.position[0].y;
        }
    }
    let y = world.bodies[body].structure.position[0].y;
    assert!(floating > 0.3, "it floated high first: {floating}");
    assert!(y < -2.0, "it sank: {y} m, {} m³ inside", flood.water);
}
