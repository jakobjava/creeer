//! Dashpots: a load linear in the motion, re-linearised each step as air's would be, against
//! its exponential decay; stable where the same load taken explicitly explodes.

use super::*;
use creeer_math::det;

const DT: f64 = 1.0 / 60.0;

/// The load −c·v and torque −k·ω at the step's start, with their slopes.
fn linear_drag(s: &mut Structure, c: f64, k: f64) {
    let (v, w) = (s.velocity[0], s.angular_velocity[0]);
    s.load[0] = -c * v;
    s.torque[0] = -k * w;
    let mut matrix = [[0.0; 6]; 6];
    for (i, row) in matrix.iter_mut().enumerate() {
        row[i] = if i < 3 { c } else { k };
    }
    s.dashpots = vec![Dashpot {
        node: 0,
        matrix,
        velocity: v,
        angular: w,
    }];
}

/// A 1 kg body under −1·v, spinning under −0.1·ω (I = 0.1): both decay as e^(−t), to first
/// order in h (the dashpot acts at the step's end).
#[test]
fn a_linear_dashpot_decays_exponentially() {
    for scheme in [Scheme::BackwardEuler, Scheme::Newmark] {
        let mut s = StructureBuilder::rigid_body(DVec3::ZERO, 1.0, DMat3::IDENTITY * 0.1);
        s.gravity = DVec3::ZERO;
        s.velocity[0] = DVec3::new(2.0, 0.0, -1.0);
        s.angular_velocity[0] = DVec3::new(0.0, 3.0, 0.0);
        // The drag acts from the start (Newmark's first acceleration is its).
        linear_drag(&mut s, 1.0, 0.1);
        let mut solver = ImplicitSolver::new(&s, scheme, DT, 1);
        for _ in 0..60 {
            linear_drag(&mut s, 1.0, 0.1);
            solver.step(&mut s);
        }
        let decay = det::exp(-1.0);
        let v = s.velocity[0] / DVec3::new(2.0, 0.0, -1.0).length();
        let off = (v.length() / decay - 1.0).abs();
        assert!(off < 0.01, "{scheme:?}: v {} against {decay}", v.length());
        let w = s.angular_velocity[0].y / 3.0;
        assert!(
            (w / decay - 1.0).abs() < 0.01,
            "{scheme:?}: ω {w} against {decay}"
        );
        assert!(s.angular_velocity[0].x.abs() < 1e-12 && s.angular_velocity[0].z.abs() < 1e-12);
    }
}

/// A 10 g body in a dashpot of 100 N·s/m (h·c/m = 167: explicitly, each step would multiply
/// its speed by −166): it stops, monotonically, and the guard takes nothing it did not give.
#[test]
fn a_stiff_dashpot_stops_a_light_body() {
    let mut s = StructureBuilder::rigid_body(DVec3::ZERO, 0.01, DMat3::IDENTITY * 1e-6);
    s.gravity = DVec3::ZERO;
    s.velocity[0] = DVec3::new(10.0, 0.0, 0.0);
    linear_drag(&mut s, 100.0, 0.0);
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    let mut last = 10.0;
    for _ in 0..30 {
        linear_drag(&mut s, 100.0, 0.0);
        solver.step(&mut s);
        let v = s.velocity[0].x;
        assert!(
            (0.0..last).contains(&v) || v.abs() < 1e-12,
            "{v} after {last}"
        );
        last = v;
    }
    assert!(last < 1e-12, "{last}");
    // Exactly it drifts v₀·m/c = 1 mm; Newmark's trapezoid moves it h·(v₀ + v₁)/2 in the step
    // its speed collapses (84 mm), no more after.
    let first = DT * 0.5 * (10.0 + 10.0 / (1.0 + 100.0 * DT / 0.01));
    assert!(
        (s.position[0].x - first).abs() < 2e-3,
        "{}",
        s.position[0].x
    );
}

/// On a member's end, a dashpot damps a cantilever's tip (ζ ≈ 0.3): from 1 m/s it comes to
/// rest at its static deflection (converged tightly: the default tolerance leaves 10⁻⁴ m/s).

#[test]
fn a_dashpot_on_a_structure_damps_it() {
    let mut b = StructureBuilder::new();
    let root = b.node(DVec3::ZERO);
    let tip = b.node(DVec3::new(1.0, 0.0, 0.0));
    b.support(root, Support::Clamped);
    b.beam(root, tip, Section::solid_round(0.02), section::STEEL_S235);
    let mut s = b.build();
    s.velocity[tip] = DVec3::new(0.0, 1.0, 0.0);
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    solver.tolerance = 1e-12;
    for _ in 0..600 {
        let v = s.velocity[tip];
        s.load[tip] = -50.0 * v;
        let mut matrix = [[0.0; 6]; 6];
        for (i, row) in matrix.iter_mut().enumerate().take(3) {
            row[i] = 50.0;
        }
        s.dashpots = vec![Dashpot {
            node: tip,
            matrix,
            velocity: v,
            angular: DVec3::ZERO,
        }];
        solver.step(&mut s);
    }
    assert!(
        s.velocity[tip].length() < 1e-5,
        "{}",
        s.velocity[tip].length()
    );
}

/// Water moving with a 1 kg body (added mass 1 kg, its momentum carried): a 2 N push
/// accelerates it as 2 kg, x = t²/2 exactly (Newmark is exact for constant acceleration);
/// going into still water at 3 m/s it shares its momentum, v = m·v₀/(m + M); its water left
/// behind, it keeps its speed.
#[test]
fn added_mass_is_inertia_without_weight() {
    for scheme in [Scheme::Newmark, Scheme::BackwardEuler] {
        let mut s = StructureBuilder::rigid_body(DVec3::ZERO, 1.0, DMat3::IDENTITY);
        s.gravity = DVec3::ZERO;
        s.load[0] = DVec3::X * 2.0;
        let water = |s: &Structure| AddedMass {
            node: 0,
            matrix: DMat3::IDENTITY,
            momentum: s.velocity[0],
        };
        // (Newmark carries the body's own acceleration, F/m, the water's impulse apart.)
        let mut solver = ImplicitSolver::new(&s, scheme, DT, 1);
        for _ in 0..60 {
            s.added_mass = vec![water(&s)];
            solver.step(&mut s);
        }
        let v = s.velocity[0].x;
        assert!((v - 1.0).abs() < 1e-9, "{scheme:?}: {v} m/s after 1 s");
        if scheme == Scheme::Newmark {
            assert!((s.position[0].x - 0.5).abs() < 1e-9, "{}", s.position[0].x);
        }
    }
    let mut s = StructureBuilder::rigid_body(DVec3::ZERO, 1.0, DMat3::IDENTITY);
    s.gravity = DVec3::ZERO;
    s.velocity[0] = DVec3::new(0.0, -3.0, 0.0);
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    s.added_mass = vec![AddedMass {
        node: 0,
        matrix: DMat3::IDENTITY * 0.5,
        momentum: DVec3::ZERO,
    }];
    solver.step(&mut s);
    assert!((s.velocity[0].y + 2.0).abs() < 1e-9, "{}", s.velocity[0].y);
    s.added_mass.clear();
    solver.step(&mut s);
    assert!((s.velocity[0].y + 2.0).abs() < 1e-9, "{}", s.velocity[0].y);
}
