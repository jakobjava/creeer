//! AC grids against phasor theory: resonance, transformers, induction motors, constant-power
//! loads, and the power balance.

use crate::*;
use creeer_math::sparse::Complex;

fn c(re: f64, im: f64) -> Complex {
    Complex::new(re, im)
}

/// A series RLC (R 10 Ω, L 0.1 H, C 10 µF) across a source, swept in frequency: the current
/// peaks at f₀ = 1/(2π√(LC)) = 159.2 Hz, and falls to 1/√2 of it f₀/Q apart (Q = √(L/C)/R = 10).
#[test]
fn rlc_resonance_and_q() {
    let (r, l, cap) = (10.0, 0.1, 10e-6);
    let current = |hz: f64| {
        let mut g = Grid::new(hz);
        let (a, b) = (g.bus(), g.bus());
        g.add(Branch::Source {
            a,
            emf: c(100.0, 0.0),
            z: c(0.0, 0.0),
        });
        let line = g.add(Branch::Line {
            a,
            b,
            z: c(r, 0.0) + g.reactance(l),
        });
        g.add(Branch::Shunt {
            a: b,
            y: Complex::from(1.0) / g.capacitive(cap),
        });
        g.solve();
        // Three phases: the line's loss is 3·|I|²·R.
        (g.power(line).re / (3.0 * r)).sqrt()
    };
    let f0 = 1.0 / (2.0 * std::f64::consts::PI * (l * cap).sqrt());
    let peak = current(f0);
    assert!(current(f0 * 0.99) < peak && current(f0 * 1.01) < peak);
    let edge = |lo: f64, hi: f64, rising: bool| {
        let (mut lo, mut hi) = (lo, hi);
        for _ in 0..80 {
            let m = 0.5 * (lo + hi);
            if (current(m) < peak / 2f64.sqrt()) == rising {
                lo = m
            } else {
                hi = m
            }
        }
        lo
    };
    let (below, above) = (edge(f0 * 0.5, f0, true), edge(f0, f0 * 2.0, false));
    let q = f0 / (above - below);
    assert!((q / ((l / cap).sqrt() / r) - 1.0).abs() < 1e-6, "Q {q}");
}

/// A 250 kVA, 11 kV / 400 V distribution transformer: 4 % impedance (X/R = 4), 0.2 % core
/// loss, 1.5 % magnetizing current. Unloaded it gives 400 V; at full load at power factor 0.8
/// its voltage falls by its regulation and it loses its copper and core losses, exactly as its
/// equivalent circuit says.
#[test]
fn transformer_ratio_and_losses() {
    let (rating, v1, v2) = (250e3, 11e3, 400.0);
    let base = v1 * v1 / rating;
    let z = c(0.04 / 17f64.sqrt(), 0.04 * 4.0 / 17f64.sqrt()) * Complex::from(base);
    let y = c(0.002, -0.015) / Complex::from(base);
    let run = |load: Complex| {
        let mut g = Grid::new(50.0);
        let (hv, lv) = (g.bus(), g.bus());
        g.add(Branch::Source {
            a: hv,
            emf: c(v1, 0.0),
            z: c(0.0, 0.0),
        });
        let t = g.add(Branch::Transformer {
            a: hv,
            b: lv,
            ratio: v1 / v2,
            z,
            y,
        });
        g.add(Branch::Load { a: lv, power: load });
        let report = g.step(1.0);
        assert!(report.converged);
        (g.voltage(lv).0, g.power(t).re, g)
    };
    let (open, core, _) = run(c(0.0, 0.0));
    assert!((open / v2 - 1.0).abs() < 1e-9, "{open} V unloaded");
    assert!(
        (core / (0.002 * rating) - 1.0).abs() < 1e-9,
        "core loss {core} W"
    );
    let (loaded, losses, g) = run(c(0.8 * rating, 0.6 * rating));
    // The same circuit by hand, per phase, referred to the primary: V₂' = V₁ − z·I with
    // I = conj(S/V₂'), solved by fixed point.
    let (vp, s) = (
        Complex::from(v1 / 3f64.sqrt()),
        c(0.8 * rating, 0.6 * rating) / Complex::from(3.0),
    );
    let mut v = vp;
    for _ in 0..200 {
        v = vp - z * (s / v).conj();
    }
    let expected = v.abs() * 3f64.sqrt() * v2 / v1;
    assert!(
        (loaded / expected - 1.0).abs() < 1e-9,
        "{loaded} V vs {expected}"
    );
    let i = (s / v).conj();
    let copper = 3.0 * i.norm_squared() * z.re;
    assert!(
        (losses - copper - core * 1.0).abs() < 1e-6 * losses,
        "{losses} W"
    );
    // About 3.3 % regulation at 0.8 lagging (ε_r·cos φ + ε_x·sin φ).
    assert!(
        (1.0 - loaded / v2 - 0.0330).abs() < 0.002,
        "{}",
        1.0 - loaded / v2
    );
    assert!(g.ledger().relative_imbalance(g.stored()) < 1e-9);
}

/// The 7.5 kW motor's torque against slip, as the grid solves it, against the textbook's
/// Thevenin form 3·V_th²·(R₂/s)/(ω_s·((R_th + R₂/s)² + (X_th + X₂)²)); and its speed under
/// a load settling where its torque meets it.
#[test]
fn induction_motor_torque_slip() {
    let m = InductionMotor::KW7_5;
    let vph = 400.0 / 3f64.sqrt();
    let jxm = c(0.0, m.xm);
    let zs = c(m.r1, m.x1);
    let vth = Complex::from(vph) * jxm / (zs + jxm);
    let zth = zs * jxm / (zs + jxm);
    let ws = m.synchronous(50.0);
    for s in [1.0, 0.5, 0.2, 0.05, 0.03, 0.01] {
        let textbook = 3.0 * vth.norm_squared() * (m.r2 / s)
            / (ws
                * ((zth.re + m.r2 / s) * (zth.re + m.r2 / s) + (zth.im + m.x2) * (zth.im + m.x2)));
        let ours = m.torque(Complex::from(vph), s, 50.0);
        assert!(
            (ours / textbook - 1.0).abs() < 1e-12,
            "s = {s}: {ours} vs {textbook}"
        );
    }
    // Rated: about 49 N·m near 1450 rpm; breakdown about 3×.
    let mut g = Grid::new(50.0);
    let bus = g.bus();
    g.add(Branch::Source {
        a: bus,
        emf: c(400.0, 0.0),
        z: c(0.0, 0.0),
    });
    let motor = g.add(Branch::Motor {
        a: bus,
        model: m,
        speed: 0.0,
        load: 0.0,
    });
    g.set_load(motor, 49.0);
    for _ in 0..600 {
        g.step(1.0 / 60.0);
    }
    let rpm = g.speed(motor) * 60.0 / (2.0 * std::f64::consts::PI);
    assert!((1430.0..1470.0).contains(&rpm), "{rpm} rpm");
    let s = m.slip(g.speed(motor), 50.0);
    let balance = m.torque(Complex::from(vph), s, 50.0) - 49.0 - m.friction * g.speed(motor);
    assert!(balance.abs() < 1e-6, "{balance} N·m");
    assert!(g.ledger().relative_imbalance(g.stored()) < 1e-9);
}

/// A constant-power load (30 kW, unity) at the end of a 0.5 Ω line from an ideal 400 V
/// source: its voltage solves V² − E·V + P·R = 0 per phase.
#[test]
fn constant_power_load() {
    let mut g = Grid::new(50.0);
    let (a, b) = (g.bus(), g.bus());
    g.add(Branch::Source {
        a,
        emf: c(400.0, 0.0),
        z: c(0.0, 0.0),
    });
    g.add(Branch::Line {
        a,
        b,
        z: c(0.5, 0.0),
    });
    g.add(Branch::Load {
        a: b,
        power: c(30e3, 0.0),
    });
    let report = g.step(1.0);
    assert!(report.converged, "{report:?}");
    let (e, p) = (400.0 / 3f64.sqrt(), 10e3);
    let v = 0.5 * (e + (e * e - 4.0 * p * 0.5).sqrt());
    assert!(
        (g.voltage(b).0 / (v * 3f64.sqrt()) - 1.0).abs() < 1e-10,
        "{}",
        g.voltage(b).0
    );
    let (supplied, dissipated, delivered, _) = g.ledger().totals();
    assert!((delivered - 30e3).abs() < 1e-6 && (supplied - dissipated - delivered).abs() < 1e-6);
}

/// A grid with everything: a generator behind its impedance, a step-down transformer, lines,
/// loads, a capacitor bank and two motors starting: what the generator gives is what the rest
/// lose, take, and store in rotors.
#[test]
fn grid_power_balance() {
    let mut g = Grid::new(50.0);
    let (plant, hv, lv, far) = (g.bus(), g.bus(), g.bus(), g.bus());
    g.add(Branch::Source {
        a: plant,
        emf: c(11.2e3, 0.0),
        z: c(0.5, 4.0),
    });
    g.add(Branch::Line {
        a: plant,
        b: hv,
        z: c(1.0, 2.0),
    });
    let base = 11e3 * 11e3 / 250e3;
    g.add(Branch::Transformer {
        a: hv,
        b: lv,
        ratio: 11e3 / 400.0,
        z: c(0.0097, 0.0388) * Complex::from(base),
        y: c(0.002, -0.015) / Complex::from(base),
    });
    g.add(Branch::Line {
        a: lv,
        b: far,
        z: c(0.02, 0.01),
    });
    g.add(Branch::Load {
        a: far,
        power: c(60e3, 20e3),
    });
    g.add(Branch::Shunt {
        a: far,
        y: Complex::from(1.0) / g.capacitive(400e-6),
    });
    for k in 0..2 {
        let motor = g.add(Branch::Motor {
            a: far,
            model: InductionMotor::KW7_5,
            speed: 0.0,
            load: 0.0,
        });
        g.set_load(motor, 20.0 * f64::from(k + 1));
    }
    for _ in 0..300 {
        assert!(g.step(1.0 / 60.0).converged);
    }
    let rel = g.ledger().relative_imbalance(g.stored());
    assert!(rel < 1e-9, "{rel:e}");
}
