//! Validation against closed forms: torsional vibration, gear trains, clutch engagement,
//! spin-down, belt slip, and the books (energy, angular momentum).

use crate::{Drivetrain, Element, Grip, HOUSING, belt_capacity};
use creeer_math::det;
use creeer_math::glam::DVec3;

fn closes(d: &Drivetrain) -> f64 {
    d.ledger().relative_imbalance(d.stored())
}

fn dissipated(d: &Drivetrain) -> f64 {
    d.ledger().totals().1
}

/// Two flywheels on a shaft ring at √(k·(J₁ + J₂)/(J₁·J₂)); momentum stays put.
#[test]
fn two_inertias_ring_at_their_natural_frequency() {
    let (j1, j2, k) = (0.02, 0.05, 500.0);
    let mut d = Drivetrain::new();
    let a = d.part(j1, DVec3::Z);
    let b = d.part(j2, DVec3::Z);
    d.add(Element::Shaft {
        a,
        b,
        stiffness: k,
        damping: 0.0,
    });
    d.set_speed(a, 1.0);
    let h = 1e-5;
    let natural = (k * (j1 + j2) / (j1 * j2)).sqrt();
    let mut crossings = Vec::new();
    let mut last = 1.0;
    let mut t = 0.0;
    while crossings.len() < 11 {
        d.step(h, DVec3::ZERO);
        t += h;
        let rel = d.speed(a) - d.speed(b);
        if last > 0.0 && rel <= 0.0 || last < 0.0 && rel >= 0.0 {
            crossings.push(t - h * rel / (rel - last));
        }
        last = rel;
    }
    let period = (crossings[10] - crossings[0]) / 5.0;
    // Backward Euler's own period, 2π·h/atan(ω·h), within 2e-5 of the exact one here.
    let exact = std::f64::consts::TAU / natural;
    assert!(
        (period / exact - 1.0).abs() < 1e-4,
        "period {period} against {exact}"
    );
    let momentum = j1 * d.speed(a) + j2 * d.speed(b);
    assert!((momentum - j1).abs() < 1e-14, "momentum {momentum}");
    assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
}

/// A motor through a 95 % reduction gear into a viscous load: in steady state the load
/// takes 95 % of the motor's power, the gear the rest.
#[test]
fn a_gear_train_passes_its_efficiency() {
    let mut d = Drivetrain::new();
    let motor = d.part(0.01, DVec3::Z);
    let load = d.part(0.5, DVec3::Z);
    let gear = d.add(Element::Gear {
        a: motor,
        b: load,
        ratio: -1.0 / 3.0,
        efficiency: 0.95,
        backlash: 0.0,
    });
    let viscous = 0.2;
    d.add(Element::Bearing {
        a: load,
        viscous,
        coulomb: 0.0,
    });
    d.set_torque(motor, 2.0);
    // Its time constant is (J_m + J_l·r²/η)/(c·r²/η) = 2.9 s: 60 s is 20 of them.
    for _ in 0..60_000 {
        d.step(1e-3, DVec3::ZERO);
    }
    let (wm, wl) = (d.speed(motor), d.speed(load));
    assert!((wl / wm + 1.0 / 3.0).abs() < 1e-14, "ratio {}", wl / wm);
    let out = viscous * wl * wl;
    assert!(
        (out / (2.0 * wm) - 0.95).abs() < 1e-6,
        "efficiency {}",
        out / (2.0 * wm)
    );
    assert!(
        (d.carried(gear) - 2.0).abs() < 1e-6,
        "carried {}",
        d.carried(gear)
    );
    assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));

    // Overrun: the load drives the motor back; the gear still loses 5 %, now of the power
    // coming from the load.
    d.set_torque(motor, -0.5);
    let before = dissipated(&d);
    let held = d.stored();
    for _ in 0..200 {
        d.step(1e-3, DVec3::ZERO);
    }
    assert!(dissipated(&d) > before);
    assert!(d.stored() < held);
    assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
}

/// A clutch engaging two flywheels loses ½·J₁J₂/(J₁ + J₂)·Δω² as heat, however hard it
/// grips; it then stays stuck.
#[test]
fn a_clutch_engagement_loses_the_reduced_inertias_energy() {
    let (j1, j2, w0) = (0.2, 0.3, 100.0);
    let expected = 0.5 * j1 * j2 / (j1 + j2) * w0 * w0;
    for capacity in [5.0, 50.0, 1e9] {
        let mut d = Drivetrain::new();
        let a = d.part(j1, DVec3::Z);
        let b = d.part(j2, DVec3::Z);
        let clutch = d.add(Element::Clutch { a, b, capacity });
        d.set_speed(a, w0);
        let mut locked = None;
        for i in 0..20_000 {
            d.step(1e-3, DVec3::ZERO);
            match (d.grip(clutch), locked) {
                (Grip::Stuck, None) => locked = Some(i),
                (Grip::Slipping(_), Some(_)) => panic!("it slipped again at {capacity} N·m"),
                _ => {}
            }
        }
        // Locked when the relative speed, falling at C·(1/J₁ + 1/J₂), reached zero.
        let lock = w0 / (capacity * (1.0 / j1 + 1.0 / j2)) / 1e-3;
        assert!(
            (locked.unwrap() as f64 - lock).abs() <= 1.0,
            "locked at step {locked:?}, expected {lock}"
        );
        let w = j1 * w0 / (j1 + j2);
        assert!((d.speed(a) - w).abs() < 1e-9 && (d.speed(b) - w).abs() < 1e-9);
        assert!(
            (dissipated(&d) / expected - 1.0).abs() < 1e-12,
            "lost {} of {expected}",
            dissipated(&d)
        );
        // All of it the clutch's heat (booked at the steps' midpoints: the trapezoid, exact for
        // its linear slip).
        let heat = d.take_heat(clutch);
        assert!(
            (heat / expected - 1.0).abs() < 1e-9,
            "clutch heat {heat} of {expected}"
        );
        assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
    }
}

/// A flywheel on a bearing with viscous and Coulomb friction: J·ω̇ = −c·ω − T_c, so
/// ω = (ω₀ + T_c/c)·e^(−c·t/J) − T_c/c, stopping at t = J/c·ln(1 + c·ω₀/T_c); then it
/// stays stopped (no chattering about zero).
#[test]
fn a_flywheel_spins_down_and_stays_stopped() {
    let (j, c, tc, w0) = (1.0, 0.01, 0.5, 100.0);
    let mut d = Drivetrain::new();
    let f = d.part(j, DVec3::Z);
    let bearing = d.add(Element::Bearing {
        a: f,
        viscous: c,
        coulomb: tc,
    });
    d.set_speed(f, w0);
    let h = 1e-3;
    let stop = j / c * det::ln(1.0 + c * w0 / tc);
    let mut stopped = None;
    for i in 1..=150_000 {
        let report = d.step(h, DVec3::ZERO);
        assert!(report.settled && report.rounds <= 3, "step {i}: {report:?}");
        let t = i as f64 * h;
        if i == 50_000 {
            let exact = (w0 + tc / c) * det::exp(-c * t / j) - tc / c;
            assert!(
                (d.speed(f) / exact - 1.0).abs() < 1e-4,
                "{} against {exact}",
                d.speed(f)
            );
        }
        if d.grip(bearing) == Grip::Stuck && stopped.is_none() {
            stopped = Some(t);
        }
        if stopped.is_some() {
            assert!(
                d.speed(f).abs() < 1e-12,
                "it moved again at {t} s: {}",
                d.speed(f)
            );
            assert_eq!(d.grip(bearing), Grip::Stuck);
        }
    }
    let stopped = stopped.expect("it stopped");
    assert!(
        (stopped - stop).abs() < 2.0 * h,
        "stopped at {stopped}, expected {stop}"
    );
    assert!((dissipated(&d) / (0.5 * j * w0 * w0) - 1.0).abs() < 1e-12);
    assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
}

/// A brake holds what it can: below its capacity the part never creeps.
#[test]
fn a_brake_holds_below_its_capacity_and_slips_above() {
    let mut d = Drivetrain::new();
    let p = d.part(0.1, DVec3::Z);
    let brake = d.add(Element::Clutch {
        a: p,
        b: HOUSING,
        capacity: 5.0,
    });
    d.set_torque(p, 4.9);
    for _ in 0..1000 {
        let report = d.step(1e-3, DVec3::ZERO);
        assert_eq!(report.rounds, 1);
        assert_eq!(d.speed(p), 0.0);
    }
    assert!(
        (d.carried(brake) - 4.9).abs() < 1e-12,
        "carried {}",
        d.carried(brake)
    );
    d.set_torque(p, 6.0);
    d.step(1e-3, DVec3::ZERO);
    assert_eq!(d.grip(brake), Grip::Slipping(1));
    assert!(
        (d.speed(p) - 1e-3 * 1.0 / 0.1).abs() < 1e-12,
        "accelerates at (6 − 5)/J"
    );
    // Released, then re-applied: it stops the part and holds it.
    d.set_torque(p, 0.0);
    for _ in 0..1000 {
        d.step(1e-3, DVec3::ZERO);
    }
    assert_eq!(d.speed(p), 0.0);
    assert_eq!(d.grip(brake), Grip::Stuck);
    assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
}

/// A V-belt drive: it carries up to its capstan capacity, then slips at exactly that torque.
#[test]
fn a_belt_slips_at_its_capstan_limit() {
    let capacity = belt_capacity(500.0, 0.05, std::f64::consts::PI, 0.3);
    // (T₁ − T₂)·r with T₁ + T₂ = 1000 N, T₁/T₂ = e^(0.3π).
    let e = det::exp(0.3 * std::f64::consts::PI);
    assert!((capacity - 1000.0 * (e - 1.0) / (e + 1.0) * 0.05).abs() < 1e-12);
    for ratio in [0.5, -0.5] {
        let mut d = Drivetrain::new();
        let driver = d.part(0.01, DVec3::Z);
        let driven = d.part(0.04, DVec3::Z);
        let belt = d.add(Element::Belt {
            a: driver,
            b: driven,
            ratio,
            efficiency: 0.97,
            capacity,
        });
        d.add(Element::Clutch {
            a: driven,
            b: HOUSING,
            capacity: 100.0,
        });
        // Below its capacity, against a held pulley: nothing turns.
        d.set_torque(driver, 0.9 * capacity);
        d.step(1e-3, DVec3::ZERO);
        assert_eq!(d.grip(belt), Grip::Stuck);
        assert_eq!(d.speed(driver), 0.0);
        // Above it: the driver spins up against the slipping belt's capacity.
        d.set_torque(driver, capacity + 3.0);
        for _ in 0..100 {
            d.step(1e-3, DVec3::ZERO);
        }
        assert_eq!(d.grip(belt), Grip::Slipping(1));
        assert!(
            (d.carried(belt) - capacity).abs() < 1e-12,
            "carried {}",
            d.carried(belt)
        );
        assert!(
            (d.speed(driver) - 0.1 * 3.0 / 0.01).abs() < 1e-9,
            "driver at {}",
            d.speed(driver)
        );
        assert_eq!(d.speed(driven), 0.0);
        assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
    }
}

/// A gear train through a bevel (axes at right angles) with a brake and a motor, its
/// housing turning: every step, the housing's reaction and the parts' change of angular
/// momentum (seen from outside) cancel; and each step only dissipates.
#[test]
fn the_housing_takes_every_reaction() {
    let mut d = Drivetrain::new();
    let motor = d.part(0.02, DVec3::Z);
    let pinion = d.part(0.005, DVec3::X);
    let wheel = d.part(0.3, DVec3::X);
    d.add(Element::Gear {
        a: motor,
        b: pinion,
        ratio: 1.0,
        efficiency: 0.98,
        backlash: 0.0,
    });
    d.add(Element::Gear {
        a: pinion,
        b: wheel,
        ratio: -0.25,
        efficiency: 0.97,
        backlash: 0.0,
    });
    d.add(Element::Bearing {
        a: wheel,
        viscous: 0.05,
        coulomb: 0.2,
    });
    let brake = d.add(Element::Clutch {
        a: wheel,
        b: HOUSING,
        capacity: 0.0,
    });
    d.set_torque(motor, 1.5);
    let spin = DVec3::new(0.3, -0.2, 0.7);
    let h = 1e-3;
    let mut last_dissipated = 0.0;
    for i in 0..4000 {
        if i == 2000 {
            d.set_capacity(brake, 20.0);
            d.set_torque(motor, 0.0);
        }
        let before = d.momentum();
        d.step(h, spin);
        // In the housing's (turning) frame: ΔL/h + spin × L = the torque from the housing.
        let rate = (d.momentum() - before) / h + spin.cross(d.momentum());
        assert!(
            (rate + d.reaction()).length() < 1e-9 * (1.0 + rate.length()),
            "step {i}: {rate} against {}",
            d.reaction()
        );
        let now = dissipated(&d);
        assert!(now >= last_dissipated - 1e-12, "step {i} created energy");
        last_dissipated = now;
    }
    assert_eq!(d.speed(wheel), 0.0, "the brake stopped it");
    assert!(closes(&d) < 1e-12, "ledger {}", closes(&d));
}

/// A part's response to more torque, for a machine on it: exactly the step's solution.
#[test]
fn the_response_is_the_steps_own() {
    let (j, c, h) = (0.4, 0.3, 1e-3);
    let mut d = Drivetrain::new();
    let p = d.part(j, DVec3::Z);
    d.add(Element::Bearing {
        a: p,
        viscous: c,
        coulomb: 0.0,
    });
    d.set_speed(p, 12.0);
    let (free, mobility) = d.response(p, h);
    assert!((free - j * 12.0 / h / (j / h + c)).abs() < 1e-12);
    assert!((mobility - 1.0 / (j / h + c)).abs() < 1e-15);
    d.set_torque(p, 3.0);
    let predicted = free + 3.0 * mobility;
    d.step(h, DVec3::ZERO);
    assert!((d.speed(p) - predicted).abs() < 1e-12);
}
