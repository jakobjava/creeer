//! The drivetrain bench's rigs behave as their physics says, and keep their books.

use super::*;

fn run(bench: &mut DriveBench, seconds: u32) {
    for _ in 0..seconds * 60 {
        bench.tick();
    }
}

/// The gearbox rig: the motor spins the flywheel up through the gearbox; the brake, stronger
/// than the motor's stall torque through it, stops both and holds them; both ledgers close.
#[test]
fn the_gearbox_spins_up_and_the_brake_stalls_it() {
    let mut bench = DriveBench::build(Rig::Gearbox);
    bench.set(true, 0.0);
    run(&mut bench, 8);
    let (d, _, wheel, _) = bench.train();
    let motor = d.speed(bench.links[0].node);
    assert!(motor > 100.0, "the motor runs: {motor} rad/s");
    assert!((d.speed(wheel) - motor / 8.0).abs() < 1e-9 * motor);
    // Stall torque 7.1 N·m × 8 × 0.94 = 53 N·m at the flywheel: a 60 N·m brake holds it.
    bench.set(true, 60.0);
    run(&mut bench, 6);
    let (d, c, wheel, brake) = bench.train();
    assert_eq!(d.grip(brake), Grip::Stuck);
    assert!(d.speed(wheel).abs() < 1e-9, "held: {}", d.speed(wheel));
    assert!(c.current(bench.links[0].machine) > 80.0, "the motor stalls");
    assert!(c.ledger().relative_imbalance(c.stored()) < 1e-9);
    assert!(d.ledger().relative_imbalance(d.stored()) < 1e-10);
}

/// The belt rig: under a light load the belt grips (the pulley at 0.4 of the motor's speed);
/// past its capstan limit it slips and heats.
#[test]
fn the_belt_grips_then_slips_past_its_limit() {
    let mut bench = DriveBench::build(Rig::Belt);
    bench.set(true, 2.0);
    run(&mut bench, 6);
    let (d, _, pulley, belt) = bench.train();
    assert_eq!(d.grip(belt), Grip::Stuck);
    let motor = d.speed(bench.links[0].node);
    assert!((d.speed(pulley) - 0.4 * motor).abs() < 1e-9 * motor);
    bench.set(true, 14.0);
    run(&mut bench, 4);
    let (d, _, pulley, belt) = bench.train();
    assert!(
        matches!(d.grip(belt), Grip::Slipping(_)),
        "{:?}",
        d.grip(belt)
    );
    assert!(d.speed(pulley) < 0.4 * d.speed(bench.links[0].node));
    assert!(bench.traces[3].1.back().unwrap()[1] > 1.0, "the belt heats");
    assert!(d.ledger().relative_imbalance(d.stored()) < 1e-10);
}
