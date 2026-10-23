//! A post in the ground (`soil.rs`) against the rigid short post's limit (Broms 1964; Brinch
//! Hansen 1961): a short post in sand, free at its head, loaded across at height e.

use crate::soil::Soil;
use creeer_math::glam::{DMat3, DVec3};
use creeer_physics::structure::section::STEEL_S235;
use creeer_physics::structure::{ImplicitSolver, Material, Scheme, Section, StructureBuilder};

const G: f64 = 9.81;

/// A 150 mm timber post, `buried` m in sand (1,800 kg/m³, 30°) and `e` m above it, springs
/// every 0.1 m below; ramped across at its head to `load` N over 8 s and held 2 s: its head's
/// sideways travel at the end, and over its last second.
fn post(buried: f64, e: f64, load: f64) -> (f64, f64) {
    let sand = Soil {
        density: 1800.0,
        friction: 30f64.to_radians(),
        cohesion: 0.0,
    };
    let timber = Material {
        youngs_modulus: 11.0e9,
        shear_modulus: 0.69e9,
        density: 420.0,
        yield_strength: 1.0e9,
        ultimate_strength: 1.0e9,
        damping: 0.05,
        ..STEEL_S235
    };
    let d = 0.15;
    let mut b = StructureBuilder::new();
    let span = 0.1;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let below = (buried / span).round() as usize;
    let mut nodes = Vec::new();
    for k in 0..=below {
        nodes.push(b.node(DVec3::new(0.0, -buried + span * k as f64, 0.0)));
    }
    let head = b.node(DVec3::new(0.0, e, 0.0));
    nodes.push(head);
    for w in nodes.windows(2) {
        b.beam_oriented(w[0], w[1], Section::solid_round(d), timber, DVec3::X);
    }
    b.mass(head, 1.0, DMat3::IDENTITY * 0.01);
    let mut s = b.build();
    s.gravity = DVec3::ZERO;
    // Each buried node's spring stands for its share of the length (half at the ends).
    for (k, &node) in nodes.iter().enumerate().take(below + 1) {
        let z = buried - span * k as f64;
        let share = if k == 0 || k == below {
            0.5 * span
        } else {
            span
        };
        let law = sand.spring(d, z.max(0.01), share, G);
        s.anchor(node, DVec3::Y, &law);
    }
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, 1.0 / 60.0, 1);
    let mut before = 0.0;
    for tick in 0..600 {
        if tick == 540 {
            before = s.displacement(head).x;
        }
        let t = (f64::from(tick) / 480.0).min(1.0);
        s.load[head] = DVec3::X * load * t;
        solver.step(&mut s);
    }
    (s.displacement(head).x, s.displacement(head).x - before)
}

/// The ultimate pressure's slope a = 3·K_p·γ·d (N/m²) in this sand.
fn slope() -> f64 {
    let (gamma, d) = (1800.0 * G, 0.15);
    let t = creeer_math::det::tan(std::f64::consts::FRAC_PI_4 + 15f64.to_radians());
    3.0 * t * t * gamma * d
}

/// Broms' capacity: the soil's pressure in front all the way down, a force at the toe.
fn broms(buried: f64, e: f64) -> f64 {
    slope() * buried * buried * buried / (6.0 * (e + buried))
}

/// The rigid post's limit with the pressure on both sides of where it turns, at z_r (Brinch
/// Hansen's 1961 method for a uniform soil's triangle of pressure): moments about the load,
/// e·z_r² + ⅔·z_r³ = e·L²/2 + L³/3, and H = a/2·(2·z_r² − L²).
fn both_sides(buried: f64, e: f64) -> f64 {
    let target = e * buried * buried / 2.0 + buried * buried * buried / 3.0;
    let (mut lo, mut hi) = (0.0, buried);
    for _ in 0..60 {
        let z = 0.5 * (lo + hi);
        if e * z * z + 2.0 / 3.0 * z * z * z < target {
            lo = z;
        } else {
            hi = z;
        }
    }
    0.5 * slope() * (2.0 * lo * lo - buried * buried)
}

/// Below the limit the post leans (near it, by degrees: its soil flows) and comes to rest;
/// above it, it is pushed over. (Broms' toe force is
/// the cruder bound: 848 N against 543 N here; the springs take the pressure behind too.)
#[test]
fn a_short_post_in_sand_fails_at_its_limit() {
    let (buried, e) = (0.8, 1.6);
    let h = both_sides(buried, e);
    assert!(h < broms(buried, e));
    let holds = post(buried, e, 0.9 * h);
    let fails = post(buried, e, 1.1 * h);
    assert!(
        holds.0 < 0.3 && holds.1.abs() < 1e-3,
        "at 0.9 H_u ({h:.0} N) it leaned {holds:?} m"
    );
    assert!(fails.0 > 1.0, "at 1.1 H_u it leaned only {fails:?} m");
}
