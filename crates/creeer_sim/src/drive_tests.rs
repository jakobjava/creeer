//! Machines on drivetrains: a motor through a gearbox spinning up to its analytic speed and
//! stalling against a brake, a generator driven by a turbine; the same speed and the same
//! joules on both sides.

use crate::drive::{self, MachineLink};
use creeer_electric::{Circuit, DcMachine, GROUND};
use creeer_math::glam::DVec3;
use creeer_mech::{Drivetrain, Element, Grip, HOUSING};

const H: f64 = 1.0 / 60.0;

/// What the circuit exported and the drivetrain imported over a step must agree.
struct Books {
    exported: f64,
    imported: f64,
}

impl Books {
    fn of(c: &Circuit, d: &Drivetrain) -> Self {
        Self {
            exported: c.ledger().totals().2,
            imported: d.ledger().totals().3,
        }
    }
}

/// A scooter motor through a 10:1 gearbox on a wheel: free, it runs at ω = V·k/(k² + R·B)
/// (B its friction and the wheel's drag reflected through the gearbox); braked, it stalls at
/// V/R, both sides at rest together and the brake holding the motor's stall torque times
/// the gearbox.
#[test]
fn a_motor_stalls_against_a_brake_on_both_sides_at_once() {
    let m = DcMachine {
        inertia: 0.0,
        friction: 0.0,
        ..DcMachine::SCOOTER_24V
    };
    let mut c = Circuit::new();
    let plus = c.node();
    c.voltage_source(plus, GROUND, 24.0);
    let machine = c.machine(plus, GROUND, m);
    let (ratio, eta, b_motor, c_wheel) = (-0.1, 0.95, 2.0e-5, 0.05);
    let mut d = Drivetrain::new();
    let rotor = d.part(DcMachine::SCOOTER_24V.inertia, DVec3::Z);
    let wheel = d.part(0.5, DVec3::Z);
    d.add(Element::Bearing {
        a: rotor,
        viscous: b_motor,
        coulomb: 0.0,
    });
    let gear = d.add(Element::Gear {
        a: rotor,
        b: wheel,
        ratio,
        efficiency: eta,
        backlash: 0.0,
    });
    d.add(Element::Bearing {
        a: wheel,
        viscous: c_wheel,
        coulomb: 0.0,
    });
    let brake = d.add(Element::Clutch {
        a: wheel,
        b: HOUSING,
        capacity: 0.0,
    });
    let mut trains = vec![d];
    let links = [MachineLink {
        machine,
        drivetrain: 0,
        node: rotor,
    }];
    let mut mismatched = 0;
    // One coupled step; 1 if the two sides came apart in it.
    let step = |c: &mut Circuit, trains: &mut [Drivetrain]| -> u32 {
        let before = Books::of(c, &trains[0]);
        let reports = drive::step(c, trains, &links, H, &[DVec3::ZERO]);
        let after = Books::of(c, &trains[0]);
        let (w_circuit, w_train) = (c.level(machine), trains[0].speed(rotor));
        let (out, into) = (
            after.exported - before.exported,
            after.imported - before.imported,
        );
        let agree = (w_circuit - w_train).abs() <= 1e-9 * (1.0 + w_train.abs())
            && (out - into).abs() <= 1e-9 * (1.0 + into.abs());
        if !agree {
            // Only where a friction element switched within the drivetrain's step.
            assert!(
                reports[0].rounds > 1,
                "{w_circuit} vs {w_train} rad/s, {out} vs {into} J"
            );
        }
        u32::from(!agree)
    };
    for _ in 0..1200 {
        mismatched += step(&mut c, &mut trains);
    }
    let big_b = b_motor + c_wheel * ratio * ratio / eta;
    let k = m.constant;
    let free = 24.0 * k / (k * k + m.resistance * big_b);
    let w = trains[0].speed(rotor);
    assert!((w / free - 1.0).abs() < 1e-6, "{w} rad/s against {free}");
    assert!((trains[0].speed(wheel) - ratio * w).abs() < 1e-12);

    // The brake, stronger than the motor's stall torque through the gearbox.
    trains[0].set_capacity(brake, 200.0);
    for _ in 0..600 {
        mismatched += step(&mut c, &mut trains);
    }
    assert_eq!(trains[0].grip(brake), Grip::Stuck);
    assert_eq!(trains[0].speed(rotor), 0.0);
    assert!(
        c.level(machine).abs() < 1e-12,
        "the circuit's rotor stopped too: {}",
        c.level(machine)
    );
    let stall = 24.0 / m.resistance;
    assert!(
        (c.current(machine) / stall - 1.0).abs() < 1e-9,
        "{} A against {stall}",
        c.current(machine)
    );
    let held = k * stall * eta / ratio.abs();
    assert!(
        (trains[0].carried(brake).abs() - held).abs() < 1e-6 * held,
        "brake holds {}",
        trains[0].carried(brake)
    );
    assert!((trains[0].carried(gear) - k * stall).abs() < 1e-9 * k * stall);
    // The step the brake locked in: the circuit was given the slipping brake's response.
    assert!(mismatched <= 1, "{mismatched} steps apart");
    assert!(c.ledger().relative_imbalance(c.stored()) < 1e-9);
    assert!(trains[0].ledger().relative_imbalance(trains[0].stored()) < 1e-12);

    // Released: it spins up again to the same speed.
    trains[0].set_capacity(brake, 0.0);
    for _ in 0..1200 {
        mismatched += step(&mut c, &mut trains);
    }
    assert!((trains[0].speed(rotor) / free - 1.0).abs() < 1e-6);
    // Letting go is seen at once (a released brake slips from then on): still only the lock.
    assert!(mismatched <= 1, "{mismatched} steps apart");
}

/// A turbine's torque on a generator into a 2 Ω load: in steady state the turbine's power is
/// the load's, the armature's and the bearing's losses, the machine's torque the turbine's
/// less the bearing's; the circuit's exported work negative (it imports it).
#[test]
fn a_turbine_drives_a_generator() {
    let m = DcMachine {
        inertia: 0.0,
        friction: 0.0,
        ..DcMachine::SCOOTER_24V
    };
    let mut c = Circuit::new();
    let a = c.node();
    let machine = c.machine(a, GROUND, m);
    let load = c.resistor(a, GROUND, 2.0);
    let mut d = Drivetrain::new();
    let shaft = d.part(0.01, DVec3::X);
    let b = 1e-3;
    d.add(Element::Bearing {
        a: shaft,
        viscous: b,
        coulomb: 0.0,
    });
    let mut trains = vec![d];
    let links = [MachineLink {
        machine,
        drivetrain: 0,
        node: shaft,
    }];
    // Its time constant J/(b + k²/(R_a + R)) is 2.9 s: a minute is 20 of them.
    let turbine = 2.0;
    for _ in 0..3600 {
        // The turbine's torque is from outside the housing (its blades' fluid).
        trains[0].set_load(shaft, turbine);
        drive::step(&mut c, &mut trains, &links, H, &[DVec3::ZERO]);
    }
    let w = trains[0].speed(shaft);
    // Steady: T = b·ω + k·i, i = k·ω/(R_a + R): ω = T/(b + k²/(R_a + R)).
    let k = m.constant;
    let exact = turbine / (b + k * k / (m.resistance + 2.0));
    assert!((w / exact - 1.0).abs() < 1e-6, "{w} against {exact}");
    assert!((c.level(machine) - w).abs() < 1e-9 * w);
    let i = -c.current(machine);
    assert!((i - k * w / (m.resistance + 2.0)).abs() < 1e-6 * i, "{i} A");
    assert!((c.torque(machine) + k * i).abs() < 1e-12);
    // The circuit took in what the drivetrain gave out (the machine's torque against it).
    let (_, _, exported, imported) = c.ledger().totals();
    let (_, _, gave, _) = trains[0].ledger().totals();
    assert!(
        exported.abs() < 1e-12 && (imported / gave - 1.0).abs() < 1e-9,
        "{imported} J taken, {gave} J given"
    );
    let _ = load;
}
