//! Parts fastened into one structure: their mass, their motion and their joints as built.

use crate::assembly::{Fastening, LONGEST, Motion, Part, Profile, assemble};
use creeer_math::glam::DVec3;
use creeer_physics::structure::section::STEEL_S235;
use creeer_physics::structure::{JointLaw, Material, Support, statics};

/// Spruce timber along the grain (C24's: EN 338).
fn spruce() -> Material {
    Material {
        youngs_modulus: 11.0e9,
        shear_modulus: 0.69e9,
        density: 420.0,
        yield_strength: 24.0e6,
        ultimate_strength: 24.0e6,
        elongation: 0.0,
        hardening_start: 0.0,
        uniform_elongation: 0.0,
        damping: 0.01,
        ..STEEL_S235
    }
}

const NAILED: JointLaw = JointLaw {
    axial: 5.0e6,
    bearing: 20.0e3,
    withdrawal: 600.0,
    lateral: 7.0e5,
    lateral_yield: 1000.0,
    lateral_peak: 1300.0,
    slip_at_peak: 0.008,
    slip_at_failure: 0.015,
    twist: 50.0,
    twist_yield: 2.0,
    interaction: 1.0,
    damping: 0.002,
};

/// A 3 m beam along x (a 75 × 150 mm board), and a 2 m post hanging down beside its middle
/// (in z, a 100 mm log), nailed where they cross.
fn tee(motions: [Motion; 2]) -> (Vec<Part>, Vec<Fastening>) {
    let beam = Part {
        start: DVec3::new(-1.5, 0.0, 0.0),
        end: DVec3::new(1.5, 0.0, 0.0),
        up: DVec3::Y,
        profile: Profile::Board {
            width: 0.15,
            depth: 0.075,
        },
        material: spruce(),
        motion: motions[0],
    };
    let between = 0.5 * 0.075 + 0.05;
    let post = Part {
        start: DVec3::new(0.0, 0.0, between),
        end: DVec3::new(0.0, -2.0, between),
        up: DVec3::X,
        profile: Profile::Round {
            start: 0.1,
            end: 0.1,
        },
        material: spruce(),
        motion: motions[1],
    };
    let nailed = Fastening {
        a: 0,
        b: 1,
        on_a: 1.5,
        on_b: 0.0,
        law: NAILED,
    };
    (vec![beam, post], vec![nailed])
}

#[test]
fn an_assembly_weighs_its_parts_and_splits_them_into_members() {
    let (parts, fastenings) = tee([Motion::default(); 2]);
    let built = assemble(&parts, &fastenings).expect("builds");
    let s = &built.structure;
    let board = 420.0 * 3.0 * 0.15 * 0.075;
    let log = 420.0 * 2.0 * std::f64::consts::PI * 0.05 * 0.05;
    assert!(
        (s.total_mass() / (board + log) - 1.0).abs() < 1e-12,
        "{}",
        s.total_mass()
    );
    // The beam at its ends, its joint and every metre (4 elements), the post in 2, and one
    // joint between them.
    assert_eq!(built.nodes[0].len(), 5);
    assert_eq!(built.nodes[1].len(), 3);
    assert_eq!(s.element_count(), 4 + 2 + 1);
    let longest = (built.nodes.iter().flat_map(|n| n.windows(2)))
        .map(|w| w[1].0 - w[0].0)
        .fold(0.0, f64::max);
    assert!(longest <= LONGEST + 1e-12);
    // Every element's two halves collide on its nodes.
    assert_eq!(built.shapes.len(), 2 * 6);
}

/// Built from parts moving and turning, the structure moves on as they did: the same
/// momentum, each node with its part's rigid motion.
#[test]
fn an_assembly_keeps_its_parts_momentum() {
    let beam = Motion {
        centre: DVec3::ZERO,
        velocity: DVec3::new(0.3, -0.1, 0.0),
        spin: DVec3::new(0.0, 0.2, 0.1),
    };
    let post = Motion {
        centre: DVec3::new(0.0, -1.0, 0.0875),
        velocity: DVec3::new(-0.2, 0.0, 0.4),
        spin: DVec3::new(0.5, 0.0, 0.0),
    };
    let (parts, fastenings) = tee([beam, post]);
    let built = assemble(&parts, &fastenings).expect("builds");
    let s = &built.structure;
    let momentum: DVec3 = (s.masses().iter().zip(&s.velocity))
        .map(|(m, v)| *v * *m)
        .sum();
    // Each part's momentum is its mass at its middle's velocity (its density uniform).
    let expected: DVec3 = parts
        .iter()
        .map(|p| {
            let mass = parts_mass(p);
            p.motion.at(0.5 * (p.start + p.end)) * mass
        })
        .sum();
    assert!(
        (momentum - expected).length() < 1e-12,
        "{momentum} against {expected}"
    );
}

fn parts_mass(p: &Part) -> f64 {
    let length = (p.end - p.start).length();
    match p.profile {
        Profile::Board { width, depth } => p.material.density * length * width * depth,
        Profile::Round { start, .. } => {
            p.material.density * length * std::f64::consts::PI * 0.25 * start * start
        }
    }
}

/// The beam clamped at both ends, 500 N on the post's foot: the post's top slips past the
/// beam's middle by F/K, the nails' slip modulus.
#[test]
fn a_fastened_joint_slips_as_its_law_says() {
    let (parts, fastenings) = tee([Motion::default(); 2]);
    let mut built = assemble(&parts, &fastenings).expect("builds");
    let (beam, post) = (&built.nodes[0], &built.nodes[1]);
    let (left, right, middle) = (beam[0].1, beam[beam.len() - 1].1, beam[2].1);
    let (top, foot) = (post[0].1, post[post.len() - 1].1);
    let s = &mut built.structure;
    s.set_support(left, Support::Clamped);
    s.set_support(right, Support::Clamped);
    s.gravity = DVec3::ZERO;
    s.load[foot] = DVec3::NEG_Y * 500.0;
    let (d, r) = statics::linear(s).expect("held");
    // The post hangs beside the beam's axis, so the load twists the beam and the post's top
    // swings with it: its slip is what is left past the beam node's own turn.
    let arm = s.position[top] - s.position[middle];
    let slip = (d[top] - d[middle] - r[middle].cross(arm)).y;
    let expected = -500.0 / NAILED.lateral;
    // Measured 3 % over F/K: the post turns against the beam under the twist, and the joint's
    // equivalent member bends a little with it (Φ = 100, ADR-0033's limits).
    assert!(
        (slip / expected - 1.0).abs() < 0.04,
        "slip {slip} m, F/K {expected} m"
    );
}

#[test]
fn a_fastening_off_its_part_is_refused() {
    let (parts, mut fastenings) = tee([Motion::default(); 2]);
    fastenings[0].on_a = 3.5;
    assert!(assemble(&parts, &fastenings).is_err());
}
