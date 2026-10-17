//! Gear sets: planetary sets and differentials against Willis' equation, backlash against
//! the plastic impact, redundant and jammed constraints, and loads from outside.

use crate::{Drivetrain, Element, Grip, HOUSING};
use creeer_math::glam::DVec3;

fn closes(d: &Drivetrain) -> f64 {
    d.ledger().relative_imbalance(d.stored())
}

fn run(d: &mut Drivetrain, steps: usize, h: f64) {
    for _ in 0..steps {
        let report = d.step(h, DVec3::ZERO);
        assert!(report.settled, "{report:?}");
    }
}

/// A planetary reduction (sun in, ring held, carrier out) gears down by 1 + Z_r/Z_s, and
/// loses (1 − η) of the power its meshes carry in the carrier's frame: Z_r/(Z_s + Z_r) of
/// what goes in.
#[test]
fn a_planetary_set_reduces_by_willis() {
    let (teeth, eta, torque, viscous) = (3.5, 0.97, 4.0, 0.3);
    let mut d = Drivetrain::new();
    let sun = d.part(0.002, DVec3::Z);
    let ring = d.part(0.05, DVec3::Z);
    let carrier = d.part(0.03, DVec3::Z);
    d.add(Element::Planetary {
        sun,
        ring,
        carrier,
        teeth,
        efficiency: eta,
    });
    let brake = d.add(Element::Clutch {
        a: ring,
        b: HOUSING,
        capacity: 1e4,
    });
    d.add(Element::Bearing {
        a: carrier,
        viscous,
        coulomb: 0.0,
    });
    d.set_torque(sun, torque);
    run(&mut d, 60_000, 1e-3);
    let (ws, wc) = (d.speed(sun), d.speed(carrier));
    assert!(
        d.speed(ring).abs() < 1e-15,
        "the ring is held: {}",
        d.speed(ring)
    );
    assert!(
        (ws / wc - (1.0 + teeth)).abs() < 1e-12,
        "reduction {}",
        ws / wc
    );
    let through = 1.0 - (1.0 - eta) * teeth / (1.0 + teeth);
    let out = viscous * wc * wc;
    assert!(
        (out / (torque * ws) - through).abs() < 1e-6,
        "efficiency {} against {through}",
        out / (torque * ws)
    );
    // The ring is held against Z_r/Z_s·η of the sun's torque.
    assert!(
        (d.carried(brake).abs() - teeth * eta * torque).abs() < 1e-9,
        "brake {}",
        d.carried(brake)
    );
    assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
}

/// An open differential (a planetary set with equal side gears) splits its cage's torque
/// equally, whatever its wheels' speeds: unequal drags run them at different speeds, their
/// mean the cage's.
#[test]
fn an_open_differential_splits_its_torque_equally() {
    let mut d = Drivetrain::new();
    let left = d.part(0.8, DVec3::X);
    let right = d.part(0.8, DVec3::X);
    let cage = d.part(0.1, DVec3::X);
    d.add(Element::Planetary {
        sun: left,
        ring: right,
        carrier: cage,
        teeth: 1.0,
        efficiency: 1.0,
    });
    let (c_left, c_right) = (0.5, 2.0);
    d.add(Element::Bearing {
        a: left,
        viscous: c_left,
        coulomb: 0.0,
    });
    d.add(Element::Bearing {
        a: right,
        viscous: c_right,
        coulomb: 0.0,
    });
    d.set_torque(cage, 50.0);
    run(&mut d, 60_000, 1e-3);
    let (wl, wr, wc) = (d.speed(left), d.speed(right), d.speed(cage));
    assert!((wl + wr - 2.0 * wc).abs() < 1e-12 * wc);
    assert!(
        (c_left * wl / (c_right * wr) - 1.0).abs() < 1e-9,
        "torques {} and {}",
        c_left * wl,
        c_right * wr
    );
    assert!((c_left * wl - 25.0).abs() < 1e-6, "each side takes half");
    assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
}

/// A gear with play: the driver turns free across half its play, then meshes in a plastic
/// impact losing ½·J_a·J_b/(J_a + J_b)·Δω²; reversed, it lets go and clunks into the other
/// flank.
#[test]
fn backlash_turns_free_then_clunks() {
    let (ja, jb, play, torque) = (0.01, 0.04, 0.02, 1.0);
    let mut d = Drivetrain::new();
    let a = d.part(ja, DVec3::Z);
    let b = d.part(jb, DVec3::Z);
    let gear = d.add(Element::Gear {
        a,
        b,
        ratio: 1.0,
        efficiency: 1.0,
        backlash: play,
    });
    assert_eq!(d.grip(gear), Grip::Free);
    d.set_torque(a, torque);
    let h = 1e-5;
    let mut t = 0.0;
    while d.grip(gear) == Grip::Free {
        let before = d.ledger().totals().1;
        d.step(h, DVec3::ZERO);
        t += h;
        if d.grip(gear) == Grip::Free {
            assert_eq!(d.speed(b), 0.0, "b turns only once meshed");
            assert_eq!(d.ledger().totals().1, before, "nothing lost while free");
        }
    }
    // Half the play at T/J_a: t = √(play·J_a/T); then the impact.
    let reach = (play * ja / torque).sqrt();
    assert!(
        (t - reach).abs() < 2.0 * h,
        "meshed at {t}, expected {reach}"
    );
    assert_eq!(
        d.grip(gear),
        Grip::Meshed(-1),
        "the driver ahead: b behind by half the play"
    );
    let w = torque / ja * reach;
    let lost = 0.5 * ja * jb / (ja + jb) * w * w;
    assert!(
        (d.ledger().totals().1 / lost - 1.0).abs() < 2e-3,
        "lost {} of {lost}",
        d.ledger().totals().1
    );
    for _ in 0..1000 {
        d.step(h, DVec3::ZERO);
        assert_eq!(d.grip(gear), Grip::Meshed(-1));
        assert!((d.speed(a) - d.speed(b)).abs() < 1e-12);
    }
    // Reversed: the flank lets go at once, b coasts, a crosses the play backwards.
    d.set_torque(a, -torque);
    d.step(h, DVec3::ZERO);
    assert_eq!(d.grip(gear), Grip::Free);
    let coasting = d.speed(b);
    while d.grip(gear) == Grip::Free {
        d.step(h, DVec3::ZERO);
        if d.grip(gear) == Grip::Free {
            // (J·ω/h)/(J/h) is ω to rounding.
            assert!((d.speed(b) - coasting).abs() < 1e-15, "b coasts");
        }
    }
    assert_eq!(d.grip(gear), Grip::Meshed(1));
    assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
}

/// Redundant constraints: a brake and a stuck bearing on one shaft, two meshes in parallel.
/// One carries what the other need not; nothing moves that cannot, nothing is singular.
#[test]
fn redundant_constraints_share_and_hold() {
    let mut d = Drivetrain::new();
    let p = d.part(0.1, DVec3::Z);
    let brake = d.add(Element::Clutch {
        a: p,
        b: HOUSING,
        capacity: 10.0,
    });
    let bearing = d.add(Element::Bearing {
        a: p,
        viscous: 0.0,
        coulomb: 2.0,
    });
    d.set_torque(p, 9.0);
    run(&mut d, 100, 1e-3);
    assert_eq!(d.speed(p), 0.0);
    let held = d.carried(brake) + d.carried(bearing);
    assert!((held - 9.0).abs() < 1e-12, "held {held}");
    // Above the brake alone, within both: still held.
    d.set_torque(p, 11.0);
    run(&mut d, 100, 1e-3);
    assert_eq!(d.speed(p), 0.0);
    assert!((d.carried(brake) + d.carried(bearing) - 11.0).abs() < 1e-12);
    // Above both: it slips against both, at (13 − 12)/J.
    d.set_torque(p, 13.0);
    d.step(1e-3, DVec3::ZERO);
    assert!(
        (d.speed(p) - 1e-3 * 1.0 / 0.1).abs() < 1e-12,
        "speed {}",
        d.speed(p)
    );

    // Two meshes in parallel with the same ratio: one shaft's motion, shared.
    let mut d = Drivetrain::new();
    let a = d.part(0.01, DVec3::Z);
    let b = d.part(0.02, DVec3::Z);
    for _ in 0..2 {
        d.add(Element::Gear {
            a,
            b,
            ratio: -2.0,
            efficiency: 1.0,
            backlash: 0.0,
        });
    }
    d.set_torque(a, 1.0);
    run(&mut d, 1000, 1e-3);
    assert!((d.speed(b) + 2.0 * d.speed(a)).abs() < 1e-12);
    let j = 0.01 + 0.02 * 4.0;
    assert!(
        (d.speed(a) - 1.0 / j).abs() < 1e-9,
        "accelerates as one: {}",
        d.speed(a)
    );
    // ... and with different ratios they jam: nothing turns, the meshes hold the torque.
    let mut d = Drivetrain::new();
    let a = d.part(0.01, DVec3::Z);
    let b = d.part(0.02, DVec3::Z);
    d.add(Element::Gear {
        a,
        b,
        ratio: -2.0,
        efficiency: 1.0,
        backlash: 0.0,
    });
    d.add(Element::Gear {
        a,
        b,
        ratio: -3.0,
        efficiency: 1.0,
        backlash: 0.0,
    });
    d.set_torque(a, 1.0);
    run(&mut d, 100, 1e-3);
    assert!(d.speed(a).abs() < 1e-12 && d.speed(b).abs() < 1e-12);
    assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
}

/// A torque from outside (the ground on a wheel) does not push on the housing; a motor's
/// does, against its stator.
#[test]
fn a_load_does_not_react_on_the_housing() {
    let mut d = Drivetrain::new();
    let wheel = d.part(1.2, DVec3::Y);
    d.set_load(wheel, -30.0);
    d.step(1e-3, DVec3::ZERO);
    assert_eq!(d.reaction(), DVec3::ZERO);
    assert!((d.speed(wheel) + 30.0 * 1e-3 / 1.2).abs() < 1e-15);
    d.set_load(wheel, 0.0);
    d.set_torque(wheel, 30.0);
    d.step(1e-3, DVec3::ZERO);
    assert!(
        (d.reaction() + DVec3::Y * 30.0).length() < 1e-12,
        "{}",
        d.reaction()
    );
    assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
}
