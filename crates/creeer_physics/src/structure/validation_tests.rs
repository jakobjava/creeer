//! Mechanics validation suite (plan.md §6): structures against analytic results.
//! Every tolerance is justified next to its test; measured values are in the comments.

use super::*;
use crate::spike::oscillation;
use section::STEEL_S235;

const DT: f64 = 1.0 / 60.0;

/// Steel that never yields: for tests of elastic theory (large deflections and buckling
/// take real steel far past its yield stress).
const ELASTIC_STEEL: Material = Material {
    yield_strength: f64::INFINITY,
    ..STEEL_S235
};

/// A horizontal cantilever along +x, clamped at x = 0, with `elements` equal elements.
fn cantilever(length: f64, elements: usize, section: Section, material: Material) -> Structure {
    let mut b = StructureBuilder::new();
    #[allow(clippy::cast_precision_loss)]
    let nodes: Vec<NodeId> = (0..=elements)
        .map(|i| b.node(DVec3::X * (length * i as f64 / elements as f64)))
        .collect();
    b.support(nodes[0], Support::Clamped);
    for pair in nodes.windows(2) {
        b.beam(pair[0], pair[1], section, material);
    }
    let mut s = b.build();
    s.gravity = DVec3::ZERO;
    s
}

fn tube() -> Section {
    Section::round_tube(0.1, 0.005)
}

fn relative_error(got: f64, want: f64) -> f64 {
    (got / want - 1.0).abs()
}

/// Tip load P: δ = P·L³/(3·EI) + P·L/(κ·G·A). Beam elements are exact for point loads, so
/// the only error is rounding.
#[test]
fn cantilever_tip_deflection_matches_beam_theory() {
    let (l, p) = (4.0, -2000.0);
    let mut s = cantilever(l, 8, tube(), STEEL_S235);
    s.load[8] = DVec3::Y * p;
    let (u, _) = statics::linear(&s).expect("supported");
    let (e, g, sec) = (STEEL_S235.youngs_modulus, STEEL_S235.shear_modulus, tube());
    let want = p * l * l * l / (3.0 * e * sec.i_z) + p * l / (sec.shear_factor[0] * g * sec.area);
    assert!(relative_error(u[8].y, want) < 1e-9, "{} vs {want}", u[8].y);
}

/// Tip moment M: tip rotation M·L/EI, tip deflection M·L²/(2·EI). Tip torque T: twist
/// T·L/GJ. Axial force N: stretch N·L/EA.
#[test]
fn tip_moment_torque_and_tension_match_theory() {
    let l = 3.0;
    let sec = Section::rectangle(0.06, 0.1);
    let mut s = cantilever(l, 6, sec, STEEL_S235);
    let (e, g) = (STEEL_S235.youngs_modulus, STEEL_S235.shear_modulus);
    s.torque[6] = DVec3::new(500.0, 0.0, 800.0);
    s.load[6] = DVec3::X * 10_000.0;
    let (u, r) = statics::linear(&s).expect("supported");
    assert!(relative_error(r[6].z, 800.0 * l / (e * sec.i_z)) < 1e-9);
    assert!(relative_error(u[6].y, 800.0 * l * l / (2.0 * e * sec.i_z)) < 1e-9);
    assert!(relative_error(r[6].x, 500.0 * l / (g * sec.torsion)) < 1e-9);
    assert!(relative_error(u[6].x, 10_000.0 * l / (e * sec.area)) < 1e-9);
}

/// Large deflection (elastica, Bisshopp & Drucker 1945): a tip load with P·L²/EI = 1
/// deflects the tip by 0.30172·L down and 0.05643·L back. Linear theory would say 0.333·L
/// down and no shortening. A slender rod, so shear plays no role.
#[test]
fn large_deflection_matches_the_elastica() {
    let (l, sec) = (5.0, Section::solid_round(0.02));
    let mut s = cantilever(l, 40, sec, ELASTIC_STEEL);
    let p = STEEL_S235.youngs_modulus * sec.i_z / (l * l);
    s.load[40] = DVec3::NEG_Y * p;
    statics::nonlinear(&mut s, 200);
    let tip = s.displacement(40);
    assert!(
        relative_error(-tip.y / l, 0.30172) < 0.003,
        "down {}",
        -tip.y / l
    );
    assert!(
        relative_error(-tip.x / l, 0.05643) < 0.01,
        "back {}",
        -tip.x / l
    );
}

/// Euler buckling of a column clamped at the base, free at the top: P_cr = π²·EI/(4·L²).
/// With a tiny side load as imperfection, the side deflection stays small below P_cr and
/// becomes large above it.
#[test]
fn column_buckles_at_the_euler_load() {
    let (l, sec) = (3.0, Section::solid_round(0.03));
    let p_cr = std::f64::consts::PI * std::f64::consts::PI * STEEL_S235.youngs_modulus * sec.i_z
        / (4.0 * l * l);
    let side = |factor: f64| {
        let mut b = StructureBuilder::new();
        #[allow(clippy::cast_precision_loss)]
        let nodes: Vec<NodeId> = (0..=20)
            .map(|i| b.node(DVec3::Y * (l * i as f64 / 20.0)))
            .collect();
        b.support(nodes[0], Support::Clamped);
        for pair in nodes.windows(2) {
            b.beam(pair[0], pair[1], sec, ELASTIC_STEEL);
        }
        let mut s = b.build();
        s.gravity = DVec3::ZERO;
        s.load[20] = DVec3::new(p_cr * 1e-4, -factor * p_cr, 0.0);
        statics::nonlinear(&mut s, 300);
        s.displacement(20).x / l
    };
    let (below, above) = (side(0.95), side(1.05));
    assert!(below < 0.01, "below P_cr: side deflection {below}");
    assert!(above > 0.1, "above P_cr: side deflection {above}");
}

/// Bends a 6 m steel cantilever with a 500 N tip load into its exact (nonlinear) static
/// shape, releases it, and measures the free vibration of the tip.
/// With `converged`, every step is solved to 1e-10 (the exact answer of the time
/// discretisation); otherwise with the solver's default settings. Returns the oscillation
/// and the tip's vertical displacement at every tick.
fn released_cantilever(scheme: Scheme, converged: bool) -> (crate::spike::Oscillation, Vec<f64>) {
    released_cantilever_with(scheme, 1, converged.then_some(1e-10))
}

fn released_cantilever_with(
    scheme: Scheme,
    substeps: u32,
    tolerance: Option<f64>,
) -> (crate::spike::Oscillation, Vec<f64>) {
    let material = Material {
        damping: 0.0,
        ..STEEL_S235
    };
    let mut s = cantilever(6.0, 20, tube(), material);
    s.load[20] = DVec3::NEG_Y * 500.0;
    // Starting from the linear solution instead would stretch the members by 0.03 % (the
    // linear theory ignores the tip's foreshortening) and fire off kHz axial vibrations.
    statics::nonlinear(&mut s, 50);
    s.load[20] = DVec3::ZERO;
    let mut solver = ImplicitSolver::new(&s, scheme, DT, substeps);
    if let Some(tolerance) = tolerance {
        solver.tolerance = tolerance;
    }
    let samples: Vec<f64> = (0..600)
        .map(|_| {
            solver.step(&mut s);
            s.displacement(20).y
        })
        .collect();
    (oscillation(&samples, DT, 0.0).expect("oscillates"), samples)
}

/// f₁ = 1.8751²/(2π·L²)·√(EI/ρA) = 2.704 Hz for the released cantilever.
fn cantilever_frequency() -> f64 {
    let sec = tube();
    let ei_rho_a = STEEL_S235.youngs_modulus * sec.i_z / (STEEL_S235.density * sec.area);
    1.8751 * 1.8751 / (2.0 * std::f64::consts::PI * 36.0) * ei_rho_a.sqrt()
}

/// Newmark at 60 Hz: −0.83 % from the first bending frequency (the time step alone
/// accounts for about −0.67 %, lumped masses for the rest), and no artificial damping. The
/// requirement is ±5 % (ADR-0002).
#[test]
fn cantilever_vibrates_at_its_natural_frequency() {
    let o = released_cantilever(Scheme::Newmark, false).0;
    let want = cantilever_frequency();
    assert!(
        relative_error(o.frequency, want) < 0.01,
        "{} Hz vs {want} Hz",
        o.frequency
    );
    assert!(o.damping_ratio.abs() < 0.002, "{o:?}");
}

/// BDF2 is 3.5 % slow at the same frequency: why Newmark is the default (ADR-0004).
#[test]
fn bdf2_is_noticeably_slow_at_a_few_hertz() {
    let o = released_cantilever(Scheme::Bdf2, false).0;
    let error = o.frequency / cantilever_frequency() - 1.0;
    assert!(
        (-0.05..-0.02).contains(&error),
        "BDF2 frequency error {error:.4}"
    );
}

/// The Newton direction is only useful if the residual is exactly −∇G (times β²): checked
/// against finite differences of G in a deformed, rotated, moving state.
#[test]
fn residual_is_the_gradient_of_the_objective() {
    let mut s = cantilever(2.0, 4, tube(), ELASTIC_STEEL);
    s.gravity = STANDARD_GRAVITY;
    s.load[4] = DVec3::new(300.0, -800.0, 200.0);
    for i in 1..5 {
        #[allow(clippy::cast_precision_loss)]
        let f = i as f64;
        s.velocity[i] = DVec3::new(0.1 * f, -0.3, 0.05 * f);
        s.angular_velocity[i] = DVec3::new(0.2, 0.1 * f, -0.3);
    }
    let mut solver = ImplicitSolver::new(&s, Scheme::Bdf2, DT, 1);
    solver.step(&mut s);
    solver.step(&mut s);
    // Now evaluate G and its gradient at a state away from the minimum.
    let w = &mut solver.work;
    for i in 1..5 {
        #[allow(clippy::cast_precision_loss)]
        let f = i as f64;
        s.position[i] += DVec3::new(0.01, -0.02 * f, 0.004);
        s.orientation[i] = crate::so3::exp(DVec3::new(0.03, -0.01 * f, 0.02)) * s.orientation[i];
    }
    w.residual(&s);
    let gradient = w.direction.clone();
    let n = s.node_count();
    let probe: Vec<DVec3> = (0..2 * n)
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let f = k as f64;
            DVec3::new(0.3 - 0.1 * f, 0.2 * f - 0.5, 0.7)
        })
        .collect();
    let h = 1e-7;
    let mut value = |sign: f64| {
        for i in 0..n {
            w.trial_x[i] = s.position[i] + probe[i] * s.free_translation[i] * (h * sign);
            w.trial_q[i] =
                crate::so3::exp(probe[n + i] * s.free_rotation[i] * (h * sign)) * s.orientation[i];
        }
        w.evaluate(&s, true, false).0
    };
    let numeric = (value(1.0) - value(-1.0)) / (2.0 * h);
    let beta = w.beta;
    let analytic: f64 = -gradient
        .iter()
        .zip(&probe)
        .map(|(g, p)| g.dot(*p))
        .sum::<f64>()
        / (beta * beta);
    assert!(
        (numeric - analytic).abs() < 1e-5 * analytic.abs().max(1.0),
        "numeric {numeric}, analytic {analytic}"
    );
}

/// The solver's stopping rule must not cost accuracy: the error it leaves must be at most a
/// tenth of the time discretisation's own error, which no solver setting can remove.
/// "Truth" is the same motion with 8× smaller time steps, fully converged. Compared tick by
/// tick over ten seconds (27 periods, so small errors build up as phase drift) and in
/// frequency. Performance must come from needing fewer or cheaper iterations, never from
/// stopping early.
#[test]
fn default_tolerance_matches_the_converged_solution() {
    let deviation = |a: &[f64], b: &[f64]| {
        a.iter()
            .zip(b)
            .fold(0.0, |m: f64, (a, b)| m.max((a - b).abs()))
    };
    let (truth, truth_samples) = released_cantilever_with(Scheme::Newmark, 8, Some(1e-10));
    let (exact, exact_samples) = released_cantilever(Scheme::Newmark, true);
    let (fast, fast_samples) = released_cantilever(Scheme::Newmark, false);
    let discretisation = deviation(&exact_samples, &truth_samples);
    let solver = deviation(&fast_samples, &exact_samples);
    assert!(
        solver <= 0.1 * discretisation,
        "solver error {solver:e} m, time discretisation error {discretisation:e} m"
    );
    let (df, ds) = (
        exact.frequency - truth.frequency,
        fast.frequency - exact.frequency,
    );
    assert!(
        ds.abs() <= 0.1 * df.abs(),
        "frequency: solver {ds:e} Hz, discretisation {df:e} Hz"
    );
}

/// Complete elliptic integral of the first kind K(k), by the arithmetic-geometric mean.
fn elliptic_k(k: f64) -> f64 {
    let (mut a, mut b) = (1.0, (1.0 - k * k).sqrt());
    while (a - b).abs() > 1e-15 {
        (a, b) = (0.5 * (a + b), (a * b).sqrt());
    }
    std::f64::consts::PI / (2.0 * a)
}

/// A pendulum (1 m steel tube pinned at the top, 50 kg bob) released at 60°: the period
/// matches the exact large-amplitude solution T = 4·√(I/(m·g·d))·K(sin(θ₀/2)) of a
/// physical pendulum, which is 7.3 % longer than the small-angle formula.
#[test]
fn pendulum_period_matches_the_exact_large_amplitude_solution() {
    let (length, bob) = (1.0, 50.0);
    let section = Section::round_tube(0.04, 0.004);
    let mut b = StructureBuilder::new();
    let top = b.node(DVec3::ZERO);
    let (sin, cos) = creeer_math::det::sin_cos(std::f64::consts::FRAC_PI_3);
    // Ten elements: with one, half the rod's mass would sit at the bob (m·L²/2 instead
    // of m·L²/3), and the period would come out 0.7 % long.
    let mut previous = top;
    for k in 1..=10 {
        let node = b.node(DVec3::new(sin, -cos, 0.0) * (length * f64::from(k) / 10.0));
        b.beam(previous, node, section, STEEL_S235);
        previous = node;
    }
    let bottom = previous;
    b.support(top, Support::Pinned);
    b.mass(bottom, bob, DMat3::ZERO);
    let mut s = b.build();
    // Physical pendulum about the pin: rod (uniform) plus bob.
    let rod = STEEL_S235.density * section.area * length;
    let (m, g) = (rod + bob, STANDARD_GRAVITY.length());
    let inertia = rod * length * length / 3.0 + bob * length * length;
    let d = (rod * 0.5 + bob) * length / m;
    let small = 2.0 * std::f64::consts::PI * (inertia / (m * g * d)).sqrt();
    let want = small * 2.0 / std::f64::consts::PI * elliptic_k(0.5);
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, DT, 1);
    let samples: Vec<f64> = (0..600)
        .map(|_| {
            solver.step(&mut s);
            s.position[bottom].x
        })
        .collect();
    let o = oscillation(&samples, DT, 0.0).expect("swings");
    assert!(
        relative_error(1.0 / o.frequency, want) < 2e-3,
        "T {} vs {want}",
        1.0 / o.frequency
    );
}
