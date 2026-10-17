//! Real steel past yield: a bar pulled at a constant strain rate follows the hardening
//! curve and Cowper–Symonds' rate law (`section.rs`, `flow.rs`).

use super::section::STEEL_S235;
use super::*;
use super::{StructureBuilder, Support};
use creeer_math::det::pow;
use creeer_math::glam::DMat3;

/// A 1 m bar of 20 mm round S235, clamped at one end, its other end driven along the bar
/// at `speed` by a mass too heavy to slow, in steps of `h` seconds, until its plastic
/// strain reaches 22 %. Returns (plastic strain, plastic strain rate, axial stress) per
/// step.
fn pull(speed: f64, h: f64) -> Vec<(f64, f64, f64)> {
    let section = Section::solid_round(0.02);
    let steel = Material {
        damping: 0.0,
        ..STEEL_S235
    };
    let mut b = StructureBuilder::new();
    let a = b.node(DVec3::ZERO);
    let end = b.node(DVec3::X);
    b.support(a, Support::Clamped);
    b.support(
        end,
        Support::Custom {
            translation: DVec3::X,
            rotation: DVec3::ZERO,
        },
    );
    b.beam(a, end, section, steel);
    b.mass(end, 1.0e12, DMat3::IDENTITY);
    let mut s = b.build();
    s.gravity = DVec3::ZERO;
    s.velocity[end] = DVec3::X * speed;
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, h, 1);
    let mut samples = Vec::new();
    let mut before = 0.0;
    while before < 0.22 {
        solver.step(&mut s);
        let e = &s.elements[0];
        let strain = e.plastic[0];
        let total = s.position[end].x - 1.0;
        let stress = steel.youngs_modulus * (total - strain);
        samples.push((strain, (strain - before) / h, stress));
        before = strain;
    }
    samples
}

/// Static flow stress times Cowper–Symonds' lift at plastic strain rate `rate`, as the
/// material defines them.
fn law(strain: f64, rate: f64) -> f64 {
    let r = STEEL_S235.rate;
    let lift = if rate > r.reference {
        pow(rate / r.d, 1.0 / r.q) - pow(r.reference / r.d, 1.0 / r.q)
    } else {
        0.0
    };
    STEEL_S235.flow_stress(strain) * (1.0 + lift)
}

/// Pulled slowly (10⁻² s⁻¹, 60 Hz steps) and fast (10 s⁻¹, 0.1 ms steps), the bar flows
/// at the curve: the 235 MPa plateau to 1.5 %, hardening to 360 MPa at 18 %, flat after;
/// lifted 10 % at the slow rate and 67 % at the fast one (Cowper–Symonds for mild steel),
/// within 1 % everywhere once it flows.
#[test]
fn a_pulled_bar_follows_the_hardening_curve_and_the_rate_law() {
    for (speed, h, lift) in [(0.01, 1.0 / 60.0, 1.099), (10.0, 1.0e-4, 1.666)] {
        let samples = pull(speed, h);
        for &(strain, rate, stress) in samples.iter().filter(|s| s.0 > 0.003) {
            let want = law(strain, rate);
            assert!(
                (stress / want - 1.0).abs() < 0.01,
                "{speed} m/s at {strain:.4}: {stress:e} Pa, the law {want:e} Pa"
            );
        }
        // The lift itself, on the plateau, against Cowper–Symonds by hand.
        let plateau = samples.iter().find(|s| s.0 > 0.005).expect("flows");
        let measured = plateau.2 / STEEL_S235.yield_strength;
        assert!(
            (measured / lift - 1.0).abs() < 0.01,
            "{speed} m/s: lift {measured}"
        );
        // And the ultimate strength past the uniform elongation.
        let last = samples.last().expect("samples");
        let ultimate = STEEL_S235.ultimate_strength * lift;
        assert!(
            (last.2 / ultimate - 1.0).abs() < 0.01,
            "{speed} m/s: {} Pa at 22 %",
            last.2
        );
    }
}

/// A measure's step law (`flow.rs`): its capacity is the slope of its work, and the
/// increment it returns minimises ½·k·(x − p)² + D(|p|): perfectly plastic, hardening, and
/// rate-sensitive, from a slow step to a crash substep.
#[test]
fn a_measure_s_increment_minimises_its_step_potential() {
    use super::flow::Flow;
    let k = 6.6e7;
    for (strain, h) in [
        (0.0, 1.0 / 60.0),
        (0.05, 1.0 / 60.0),
        (0.05, 5.0e-4),
        (0.3, 5.0e-4),
    ] {
        for material in [STEEL_S235, STEEL_S235.perfectly_plastic()] {
            let flow = Flow::new(7.4e4, &material, strain, 1.0, h);
            for u in [1e-7, 1e-5, 1e-3, 1e-2] {
                let e = u * 1e-6;
                let fd = (flow.work(u + e) - flow.work(u - e)) / (2.0 * e);
                assert!((fd / flow.slope(u) - 1.0).abs() < 1e-6, "slope at {u}");
            }
            for x in [-3e-3, 5e-4, 1e-3, 2e-2] {
                let p = flow.increment(x, k);
                let total = |p: f64| 0.5 * k * (x - p) * (x - p) + flow.work(p.abs());
                let at = total(p);
                for q in [p * (1.0 + 1e-6), p * (1.0 - 1e-6), p + 1e-9, p - 1e-9, 0.0] {
                    assert!(
                        at <= total(q) + 1e-12 * at.abs(),
                        "{x}: {p} is not the minimum"
                    );
                }
            }
        }
    }
}

/// Real S235 pulled with 1.2 times its yield load (ramped over 5 s, then held) hardens to
/// carry it: it flows until its flow stress reaches the load, σ(ε) = 1.2·σ_y at 7.7 %
/// plastic strain, and stays whole (perfectly plastic steel would tear, `plastic_tests.rs`).
/// Its static strength is the tensile test's, so it creeps the last percent at about that
/// test's rate: 6.0 % after 10 s, 7.71 % after 40 s, and no more after.
#[test]
fn a_bar_loaded_past_yield_hardens_to_carry_its_load() {
    let section = Section::solid_round(0.02);
    let mut b = StructureBuilder::new();
    let a = b.node(DVec3::ZERO);
    let end = b.node(DVec3::X);
    b.support(a, Support::Clamped);
    b.beam(a, end, section, STEEL_S235);
    b.mass(end, 1000.0, DMat3::from_diagonal(DVec3::splat(10.0)));
    let mut s = b.build();
    s.gravity = DVec3::ZERO;
    let load = 1.2 * STEEL_S235.yield_strength * section.area;
    let dt = 1.0 / 60.0;
    let mut solver = ImplicitSolver::new(&s, Scheme::Newmark, dt, 1);
    for tick in 0..3000 {
        let t = f64::from(tick) * dt;
        s.load[end] = DVec3::X * load * (t / 5.0).min(1.0);
        solver.step(&mut s);
    }
    let e = &s.elements[0];
    assert!(!e.broken, "broke");
    let span = STEEL_S235.uniform_elongation - STEEL_S235.hardening_start;
    let rise = STEEL_S235.ultimate_strength - STEEL_S235.yield_strength;
    let want = STEEL_S235.hardening_start + 0.2 * STEEL_S235.yield_strength / rise * span;
    let strain = e.plastic[0];
    assert!(
        (strain / want - 1.0).abs() < 0.01,
        "plastic strain {strain}, expected {want}"
    );
}
