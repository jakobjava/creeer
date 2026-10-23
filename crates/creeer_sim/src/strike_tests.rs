//! A swing's speed follows the measured law from the measured strikes, and its blow conserves
//! momentum.

use super::*;
use crate::tools::Kind;

/// A tool of `mass` (kg) whose moment of inertia about its handle's end is `inertia`, balanced
/// `balance` m up, striking `strike` m out.
fn tool(mass: f64, inertia: f64, balance: f64, strike: f64) -> Tool {
    Tool {
        name: "test".into(),
        model: "test".into(),
        kind: Kind::Axe,
        mass,
        head: 0.5 * mass,
        length: strike,
        grip: 0.05,
        radius: 0.016,
        carry: balance,
        carry_radius: 0.016,
        gyration: (inertia / mass).sqrt(),
        strike,
        edge: None,
    }
}

/// A uniform rod of `mass` and `length`, struck with at its end.
fn rod(mass: f64, length: f64) -> Tool {
    tool(mass, mass * length * length / 3.0, 0.5 * length, length)
}

#[test]
fn the_volunteers_rods_are_swung_as_fast_as_they_swung_them() {
    // The 1 kg, 65 cm rod at its end: the mean the men reached.
    assert!((swing(&rod(1.0, 0.65)).speed - 23.9).abs() < 1e-9);
    // The others the men swung (0.5–1.5 kg, 40–90 cm): within the 14.0–35.5 m/s they reached.
    for mass in [0.5, 1.0, 1.5] {
        for length in [0.4, 0.65, 0.9] {
            let v = swing(&rod(mass, length)).speed;
            assert!((14.0..=35.5).contains(&v), "{mass} kg, {length} m: {v} m/s");
        }
    }
}

#[test]
fn a_bat_is_swung_a_little_slower_than_collegiate_batters_swing_it() {
    // A baseball bat: 0.88 kg, about 0.25 kg·m² about the knob, its sweet spot 0.71 m out.
    let bat = tool(0.88, 0.25, 0.58, 0.71);
    let v = swing(&bat).speed;
    // Trained batters reach 24.7–26.8 m/s there (Fleisig et al. 2002); the volunteers less.
    assert!((19.0..24.7).contains(&v), "{v} m/s");
}

#[test]
fn speed_falls_with_swing_weight_as_measured_and_not_with_mass() {
    let light = swing(&tool(1.0, 0.5, 0.6, 0.8));
    let heavy = swing(&tool(1.0, 1.0, 0.6, 0.8));
    assert!((heavy.spin / light.spin - det::pow(0.5, 0.27)).abs() < 1e-12);
    // The same swing-weight twice as heavy: as fast (Cross and Bower: speed independent of
    // mass at a given swing-weight).
    let massive = swing(&tool(2.0, 1.0, 0.6, 0.8));
    assert!((massive.spin - heavy.spin).abs() < 1e-12);
    // Heavier swings carry more energy for all that (n < 0.5).
    assert!(heavy.energy > light.energy);
}

#[test]
fn the_packs_tools_strike_hard_and_take_their_time() {
    let data = creeer_data::Data::base();
    let rod = swing(&rod(1.0, 0.65));
    for t in Tool::all(&data).expect("tools") {
        let s = swing(&t);
        println!(
            "{}: {:.1} rad/s, {:.1} m/s, {:.0} J, effective {:.2} kg, raised {:.2} s, struck {:.3} s",
            t.name, s.spin, s.speed, s.energy, s.mass, s.raise, s.strike
        );
        // Heavier than the volunteers' rod: slower, and the saw lighter: faster.
        let inertia = t.mass * t.gyration * t.gyration;
        assert_eq!(s.spin < rod.spin, inertia > ROD_INERTIA, "{}", t.name);
        assert!(s.mass > 0.0 && s.mass < t.mass);
        assert!((s.raise + s.strike - TO_BLOW).abs() < 1e-12 && s.strike < 0.3);
    }
}

#[test]
fn a_blow_conserves_momentum_and_bounces_by_its_restitution() {
    // Equal effective masses, no bounce: half the speed's momentum each.
    let j = impulse(10.0, 2.0, 2.0, 0.0);
    assert!((j - 10.0).abs() < 1e-12);
    // A perfect bounce off an immovable body: twice the tool's momentum.
    let j = impulse(10.0, 2.0, f64::INFINITY, 1.0);
    assert!((j - 40.0).abs() < 1e-12);
}
