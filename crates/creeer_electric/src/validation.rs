//! Devices against their equations and datasheets: diodes, LEDs, batteries, solar modules,
//! DC machines, fuses and breakers; and the ledger of a circuit with all of them.

use crate::*;
use creeer_math::det;

const TICK: f64 = 1.0 / 60.0;

/// 5 V through 1 kΩ into a 1N4148: the solution is Shockley's law with the series resistance,
/// solved independently by bisection: V = I·(R + R_s) + n·V_T·ln(1 + I/I_s).
#[test]
fn diode_i_v() {
    let d = Diode::N4148;
    let mut c = Circuit::new();
    let (top, mid) = (c.node(), c.node());
    c.voltage_source(top, GROUND, 5.0);
    c.resistor(top, mid, 1000.0);
    let diode = c.diode(mid, GROUND, d);
    let report = c.step(TICK);
    assert_eq!(report.unconverged, 0);
    let nvt = d.emission * devices::K_OVER_Q * 293.15;
    let drop = |i: f64| i * (1000.0 + d.series) + nvt * det::ln(1.0 + i / d.saturation);
    let (mut lo, mut hi) = (0.0, 5e-3);
    for _ in 0..200 {
        let m = 0.5 * (lo + hi);
        if drop(m) > 5.0 { hi = m } else { lo = m }
    }
    assert!(
        (c.current(diode) / lo - 1.0).abs() < 1e-9,
        "{} vs {lo}",
        c.current(diode)
    );
    // About 0.65 V across it at 4.3 mA (the datasheet's typical forward curve).
    assert!((0.62..0.70).contains(&c.voltage(mid)), "{}", c.voltage(mid));
    // Reversed, it blocks: nanoamps.
    let mut r = Circuit::new();
    let (top, mid) = (r.node(), r.node());
    r.voltage_source(top, GROUND, -5.0);
    r.resistor(top, mid, 1000.0);
    let back = r.diode(mid, GROUND, d);
    r.step(TICK);
    assert!(r.current(back).abs() < 1e-8, "{}", r.current(back));
}

/// A red LED on 5 V through 150 Ω: about 20 mA at 1.9 V, a fifth of its power leaving as light.
#[test]
fn led() {
    let mut c = Circuit::new();
    let (top, mid) = (c.node(), c.node());
    c.voltage_source(top, GROUND, 5.0);
    c.resistor(top, mid, 150.0);
    let led = c.diode(mid, GROUND, Diode::RED_LED);
    for _ in 0..10 {
        c.step(TICK);
    }
    let (i, v) = (c.current(led), c.across(led));
    assert!(
        (0.019..0.021).contains(&i) && (1.88..1.98).contains(&v),
        "{i} A at {v} V"
    );
    let (_, _, light, _) = c.ledger().totals();
    assert!((light / (0.2 * i * v * 10.0 * TICK) - 1.0).abs() < 1e-6);
}

/// Steps a battery discharged at constant current `amps` until its protection cuts it off:
/// the charge it gave (Ah).
fn discharge(model: Battery, amps: f64, dt: f64) -> f64 {
    let mut c = Circuit::new();
    let plus = c.node();
    let cell = c.battery(plus, GROUND, model);
    c.add(Kind::CurrentSource(amps), plus, GROUND);
    let mut steps = 0u32;
    while c.closed(cell) && steps < 2_000_000 {
        c.step(dt);
        steps += 1;
    }
    amps * f64::from(steps) * dt / 3600.0
}

/// An 18650 cell (3.2 Ah): about its rating at 0.2C, a little less at 1C and 2C (the voltage
/// sags to the cut-off sooner), as the NCR18650B's discharge curves show; and the ledger of a
/// discharge closes (chemical energy out = electrical + heat).
#[test]
fn battery_discharge() {
    let slow = discharge(Battery::li_ion_18650(), 0.64, 1.0);
    let one = discharge(Battery::li_ion_18650(), 3.2, 1.0);
    let two = discharge(Battery::li_ion_18650(), 6.4, 1.0);
    assert!((3.0..3.25).contains(&slow), "0.2C: {slow} Ah");
    assert!(
        one < slow && two < one && two > 2.6,
        "1C {one} Ah, 2C {two} Ah"
    );
    // A lead-acid battery's rate effect is far stronger: at C/1 about 55–70 % of its C/20.
    let c20 = discharge(Battery::lead_acid_12v_100ah(), 5.0, 10.0);
    let c1 = discharge(Battery::lead_acid_12v_100ah(), 100.0, 1.0);
    assert!(
        (92.0..101.0).contains(&c20) && (0.55..0.72).contains(&(c1 / c20)),
        "C/20 {c20} Ah, C/1 {c1} Ah"
    );
    let mut c = Circuit::new();
    let plus = c.node();
    c.battery(plus, GROUND, Battery::li_ion_18650());
    c.resistor(plus, GROUND, 2.0);
    for _ in 0..3600 {
        c.step(1.0);
    }
    assert!(c.ledger().relative_imbalance(c.stored()) < 1e-9);
}

/// A battery left to rest after a hard discharge recovers (its bound charge diffusing to the
/// electrodes): its open-circuit voltage climbs back.
#[test]
fn a_rested_battery_recovers() {
    let mut c = Circuit::new();
    let plus = c.node();
    let cell = c.battery(plus, GROUND, Battery::lead_acid_12v_100ah());
    let mid = c.node();
    let load = c.add(Kind::Switch, plus, mid);
    c.resistor(mid, GROUND, 0.12);
    for _ in 0..1800 {
        c.step(1.0);
    }
    c.set_closed(load, false);
    c.step(1.0);
    let just = c.voltage(plus);
    for _ in 0..7200 {
        c.step(1.0);
    }
    assert!(
        c.voltage(plus) > just + 0.05,
        "{just} → {}",
        c.voltage(plus)
    );
    assert!(c.closed(cell));
}

/// The KC200GT at standard test conditions against its datasheet: I_sc 8.21 A, V_oc 32.9 V,
/// 200 W at 26.3 V and 7.61 A (Villalva et al.'s fit reproduces them to 1 %).
#[test]
fn solar_module_i_v() {
    let at = |volts: f64, irradiance: f64, kelvin: f64| {
        let mut c = Circuit::new();
        let plus = c.node();
        let m = c.solar(plus, GROUND, SolarModule::KC200GT);
        c.set_irradiance(m, irradiance);
        c.set_temperature(m, kelvin);
        c.voltage_source(plus, GROUND, volts);
        let report = c.step(TICK);
        assert_eq!(report.unconverged, 0, "{volts} V");
        -c.current(m)
    };
    let (g, t) = (1000.0, 298.15);
    assert!(
        (at(0.0, g, t) / 8.21 - 1.0).abs() < 0.01,
        "I_sc {}",
        at(0.0, g, t)
    );
    assert!(
        (at(26.3, g, t) / 7.61 - 1.0).abs() < 0.02,
        "I_mp {}",
        at(26.3, g, t)
    );
    let (mut lo, mut hi) = (25.0, 40.0);
    for _ in 0..60 {
        let m = 0.5 * (lo + hi);
        if at(m, g, t) > 0.0 { lo = m } else { hi = m }
    }
    assert!((lo / 32.9 - 1.0).abs() < 0.01, "V_oc {lo}");
    let best = (200..300)
        .map(|k| f64::from(k) * 0.1)
        .map(|v| v * at(v, g, t))
        .fold(0.0, f64::max);
    assert!((best / 200.1 - 1.0).abs() < 0.02, "P_max {best}");
    // Half the light, half the current; hot, a lower voltage.
    assert!((at(0.0, 500.0, t) / at(0.0, g, t) - 0.5).abs() < 0.005);
    assert!(at(26.3, g, 348.15) < 0.7 * at(26.3, g, t));
}

/// A 24 V scooter motor: its speed falls linearly with the torque on it,
/// ω = (V − R·T/k)/(k + R·b/k), from 3100 rpm at no load.
#[test]
fn dc_motor_speed_torque() {
    let m = DcMachine::SCOOTER_24V;
    for load in [0.0, 1.0, 2.0, 4.0] {
        let mut c = Circuit::new();
        let plus = c.node();
        c.voltage_source(plus, GROUND, 24.0);
        let motor = c.machine(plus, GROUND, m);
        c.set_load(motor, load);
        for _ in 0..540 {
            c.step(TICK);
        }
        let (before, _, work_before, _) = c.ledger().totals();
        let (supplied_before, work_before) = (before, work_before);
        for _ in 0..60 {
            c.step(TICK);
        }
        let exact = (24.0 - m.resistance * load / m.constant)
            / (m.constant + m.resistance * m.friction / m.constant);
        assert!(
            (c.level(motor) / exact - 1.0).abs() < 1e-6,
            "{load} N·m: {} vs {exact}",
            c.level(motor)
        );
        assert!((c.torque(motor) - load - m.friction * exact).abs() < 1e-6);
        // At speed, over its last second: the shaft takes load·ω, the source gives V·I, the
        // armature's and the bearings' losses the rest.
        let (supplied, _, work, _) = c.ledger().totals();
        let current = c.torque(motor) / m.constant;
        assert!((work - work_before - load * exact).abs() < 1e-6 * (1.0 + load * exact));
        assert!(
            (supplied - supplied_before - 24.0 * current).abs() < 1e-6 * 24.0 * current.max(1.0)
        );
        assert!(c.ledger().relative_imbalance(c.stored()) < 1e-9);
    }
}

/// A 10 A fuse (pre-arcing I²t 100 A²·s) melts in 0.25 s at 20 A; a 16 A C-curve breaker
/// trips at once at 160 A, at twice its rating after its thermal time.
#[test]
fn fuses_and_breakers() {
    let mut c = Circuit::new();
    let top = c.node();
    c.voltage_source(top, GROUND, 2.0);
    let mid = c.node();
    let fuse = c.add(Kind::Fuse { melting: 100.0 }, top, mid);
    c.resistor(mid, GROUND, 0.1 - CONTACT);
    let mut t = 0.0f64;
    while c.closed(fuse) {
        c.step(0.001);
        t += 0.001;
    }
    assert!((t - 0.25).abs() < 0.002, "melted at {t} s");
    let breaker = |amps: f64| {
        let mut c = Circuit::new();
        let top = c.node();
        let mid = c.node();
        c.add(Kind::CurrentSource(-amps), top, GROUND);
        let b = c.add(
            Kind::Breaker {
                rated: 16.0,
                thermal: 30.0,
                magnetic: 7.5,
            },
            top,
            mid,
        );
        c.resistor(mid, GROUND, 1.0);
        let mut t = 0.0f64;
        while c.closed(b) && t < 100.0 {
            c.step(0.01);
            t += 0.01;
        }
        t
    };
    assert!(breaker(160.0) <= 0.011, "magnetic");
    assert!(
        (breaker(32.0) - 10.0).abs() < 0.05,
        "thermal: {}",
        breaker(32.0)
    );
    assert!(breaker(15.0) >= 100.0, "below its rating it holds");
}

/// A circuit with everything: a battery and a solar module feeding a motor and an LED through
/// a fuse and wires, a capacitor across, an inductor in series, a switch flipped now and
/// then: supplied = dissipated + passed on + held, to Newton's tolerance.
#[test]
fn power_balance() {
    let mut c = Circuit::new();
    let (bus, a, b, l) = (c.node(), c.node(), c.node(), c.node());
    c.battery(bus, GROUND, Battery::lead_acid_12v_100ah());
    let sun = c.solar(bus, GROUND, SolarModule::KC200GT);
    c.set_irradiance(sun, 300.0);
    c.add(Kind::Fuse { melting: 1e6 }, bus, a);
    c.capacitor(a, GROUND, 4.7e-3);
    c.inductor(a, l, 1e-3);
    let motor = c.machine(l, GROUND, DcMachine::SCOOTER_24V);
    c.set_load(motor, 0.5);
    let switch = c.add(Kind::Switch, a, b);
    let lamp = c.node();
    c.resistor(b, lamp, 560.0);
    c.diode(lamp, GROUND, Diode::RED_LED);
    for k in 0..600 {
        if k % 97 == 0 {
            c.set_closed(switch, k % 194 == 0);
        }
        let report = c.step(TICK);
        assert_eq!(report.unconverged, 0, "step {k}");
    }
    let rel = c.ledger().relative_imbalance(c.stored());
    assert!(rel < 1e-9, "{rel:e}");
}
