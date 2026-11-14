//! The modes against beam theory and Lamb's sphere.

use super::*;

const SPRUCE: Acoustic = Acoustic {
    youngs: 11e9,
    shear: 0.69e9,
    density: 420.0,
    loss: 0.01,
};

/// The mean of `f` over 0 to 1 (Simpson's rule).
fn mean(f: impl Fn(f64) -> f64) -> f64 {
    let n = 4000;
    let h = 1.0 / f64::from(n);
    let mut sum = f(0.0) + f(1.0);
    for k in 1..n {
        let w = if k % 2 == 1 { 4.0 } else { 2.0 };
        sum += w * f(f64::from(k) * h);
    }
    sum * h / 3.0
}

#[test]
fn the_beam_shapes_have_a_mean_square_of_one_and_their_textbook_ends() {
    for n in 0..10 {
        let bl = free_free(n);
        let ms = mean(|x| free_free_shape(bl, x).powi(2));
        assert!((ms - 1.0).abs() < 1e-6, "free–free mode {n}: {ms}");
        assert!((free_free_shape(bl, 0.0) - 2.0).abs() < 1e-6, "mode {n}");
        assert!(
            (free_free_shape(bl, 1.0).abs() - 2.0).abs() < 1e-6,
            "mode {n}"
        );
        let bl = clamped_free(n);
        let ms = mean(|x| cantilever_shape(bl, x).powi(2));
        assert!((ms - 1.0).abs() < 1e-6, "cantilever mode {n}: {ms}");
        assert!(cantilever_shape(bl, 0.0).abs() < 1e-9);
        assert!(
            (cantilever_shape(bl, 1.0).abs() - 2.0).abs() < 1e-6,
            "cantilever mode {n}: {}",
            cantilever_shape(bl, 1.0)
        );
    }
}

/// The spherical Bessel functions j₀…j₆ at x (upward recurrence, fine for x above n/2 here).
fn bessel(x: f64) -> [f64; 7] {
    let mut j = [0.0; 7];
    j[0] = x.sin() / x;
    j[1] = x.sin() / (x * x) - x.cos() / x;
    for n in 1..6 {
        j[n + 1] = (2.0 * n as f64 + 1.0) / x * j[n] - j[n - 1];
    }
    j
}

#[test]
fn the_sphere_roots_are_lambs() {
    // x·j′ₙ(x) − jₙ(x) = (n − 1)·jₙ(x) − x·jₙ₊₁(x).
    let g = |n: usize, x: f64| {
        let j = bessel(x);
        (n as f64 - 1.0) * j[n] - x * j[n + 1]
    };
    let root = |n: usize, mut a: f64, mut b: f64| {
        assert!(g(n, a) * g(n, b) < 0.0, "no root of n = {n} in [{a}, {b}]");
        for _ in 0..80 {
            let m = 0.5 * (a + b);
            if g(n, a) * g(n, m) <= 0.0 {
                b = m;
            } else {
                a = m;
            }
        }
        0.5 * (a + b)
    };
    let found = [
        root(2, 2.0, 3.0),
        root(3, 3.3, 4.4),
        root(4, 4.5, 5.6),
        root(5, 5.7, 6.8),
        root(2, 6.5, 7.6),
    ];

    for (k, (got, table)) in found.iter().zip(SPHERE_TORSION).enumerate() {
        assert!(
            (got - table).abs() < 1e-5,
            "root {k}: {got} (table {table})"
        );
    }
}

#[test]
fn a_struck_log_rings_at_its_bending_modes() {
    // A spruce log 3 m long, 0.2 m across: f₁ = (4.730)²/(2π·9)·√(E/ρ)·r/2 ≈ 101 Hz.
    let shape = Shape::Bar {
        length: 3.0,
        width: 0.2,
        depth: 0.2,
        round: true,
    };
    let mass = SPRUCE.density * shape.volume();
    let blow = Blow {
        along: 0.1,
        impulse: [0.0, 5.0, 0.0],
        contact: 2e-4,
    };
    let modes = bar(&SPRUCE, &shape, mass, &blow, 0.0);
    let lowest = modes
        .as_slice()
        .iter()
        .map(|m| m.freq)
        .fold(f32::MAX, f32::min);
    let expect = 4.730_040_7f64.powi(2) / (2.0 * PI * 9.0) * (11e9f64 / 420.0).sqrt() * 0.05;
    assert!(
        (f64::from(lowest) - expect).abs() < 0.01 * expect,
        "{lowest} vs {expect}"
    );
    // Its overtones stand at (βₙ/β₁)²: 2.756, 5.404, 8.933 times the fundamental.
    let mut freqs: Vec<f32> = modes
        .as_slice()
        .iter()
        .filter(|m| m.amp > 0.0)
        .map(|m| m.freq)
        .collect();
    freqs.sort_by(f32::total_cmp);
    for ratio in [2.756, 5.404, 8.933] {
        let want = expect * ratio;
        assert!(
            freqs
                .iter()
                .any(|&f| (f64::from(f) - want).abs() < 0.01 * want),
            "no mode near {want:.0} Hz in {freqs:?}"
        );
    }
    // Each decays as its wood damps, π·f·η.
    for m in modes.as_slice() {
        let want = PI * f64::from(m.freq) * 0.01;
        assert!((f64::from(m.decay) - want).abs() < 1e-3 * want);
    }
}

#[test]
fn struck_in_the_middle_a_bar_rings_none_of_its_odd_shaped_modes() {
    // The second bending mode is antisymmetric: still at the middle.
    assert!(free_free_shape(free_free(1), 0.5).abs() < 1e-5);
    assert!(free_free_shape(free_free(0), 0.5).abs() > 1.0);
}

#[test]
fn a_harder_blow_is_louder_and_a_softer_contact_duller() {
    let shape = Shape::Bar {
        length: 1.0,
        width: 0.05,
        depth: 0.05,
        round: false,
    };
    let mass = SPRUCE.density * shape.volume();
    let hit = |j: f64, tau: f64| {
        bar(
            &SPRUCE,
            &shape,
            mass,
            &Blow {
                along: 0.0,
                impulse: [0.0, j, 0.0],
                contact: tau,
            },
            0.0,
        )
    };
    let soft = hit(1.0, 1e-4).peak();
    let hard = hit(2.0, 1e-4).peak();
    assert!(
        (hard / soft - 2.0).abs() < 1e-6,
        "amplitude follows the impulse"
    );
    // A long contact passes the low modes and not the high ones.
    let brightness = |m: &Modes| {
        let total: f64 = m.as_slice().iter().map(|m| f64::from(m.amp)).sum();
        m.as_slice()
            .iter()
            .map(|m| f64::from(m.amp * m.freq))
            .sum::<f64>()
            / total
    };
    assert!(brightness(&hit(1.0, 3e-3)) < 0.5 * brightness(&hit(1.0, 1e-5)));
}

#[test]
fn a_nail_rings_higher_as_it_is_driven() {
    let steel = Acoustic {
        youngs: 210e9,
        shear: 80.8e9,
        density: 7850.0,
        loss: 1e-3,
    };
    let lowest = |standing: f64| {
        let m = cantilever(&steel, (standing, 0.0031, 0.02), 0.3, 1.0, 5e-5);
        m.as_slice().iter().map(|m| m.freq).fold(f32::MAX, f32::min)
    };
    // f ∝ 1/L²: from 50 mm out to 25 mm out, four times higher.
    let (long, short) = (lowest(0.05), lowest(0.025));
    assert!(
        (f64::from(short / long) - 4.0).abs() < 0.01,
        "{long} → {short}"
    );
    // 50 mm of a 3.1 mm nail: 1.875²/(2π·0.05²)·√(E/ρ)·d/4 ≈ 0.9 kHz.
    assert!((800.0..1000.0).contains(&long), "{long}");
}

#[test]
fn a_slender_box_is_a_bar_and_a_stubby_one_a_ball() {
    let (shape, order) = Shape::of_box([0.05, 1.5, 0.1]);
    assert!(matches!(shape, Shape::Bar { length, .. } if (length - 3.0).abs() < 1e-12));
    assert_eq!(order, [1, 2, 0]);
    let (shape, _) = Shape::of_box([0.1, 0.1, 0.1]);
    assert!(matches!(shape, Shape::Ball { .. }));
    assert!((shape.volume() - 0.008).abs() < 1e-12);
}
