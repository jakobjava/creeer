//! Content carried against its closed forms: a plug's front arriving after the pipe's transit
//! time, unsmeared and coming back intact when the flow turns; streams mixing by mass; a
//! stirred tank's 1 − e^(−ṁt/M); the books of every quantity.

use super::{Holding, Tracer};
use creeer_math::det;

/// A plug of tracer through a pipe of 1000 kg at 2 kg/s: none arrives before 500 s, all after,
/// the front sharp; turned round, the pipe gives back what went in, unmixed.
#[test]
fn a_plug_arrives_after_its_transit_unsmeared() {
    let mut t = Tracer::new(1);
    let (a, b) = (
        t.node(Holding::Source, 0.0, &[1.0]),
        t.node(Holding::Source, 0.0, &[0.0]),
    );
    let pipe = t.link(a, b, 1000.0, &[0.0]);
    let none = [0.0; 2];
    for _ in 0..49 {
        t.step(&[2.0], &none, 10.0);
    }
    assert_eq!(t.delivered(pipe, b), vec![0.0], "not yet at 490 s");
    t.step(&[2.0], &none, 10.0);
    t.step(&[2.0], &none, 10.0);
    assert_eq!(t.delivered(pipe, b), vec![1.0], "there at 510 s");
    // Half filled (500 kg of tracer at a, 500 clear at b), turned round: out at a comes the
    // tracer for 250 s, then the clear water that was ahead of it.
    let mut t = Tracer::new(1);
    let (a, b) = (
        t.node(Holding::Source, 0.0, &[1.0]),
        t.node(Holding::Source, 0.0, &[0.0]),
    );
    let pipe = t.link(a, b, 1000.0, &[0.0]);
    for _ in 0..25 {
        t.step(&[2.0], &none, 10.0);
    }
    t.set(a, &[0.5]);
    t.set(b, &[0.25]);
    for _ in 0..24 {
        t.step(&[-2.0], &none, 10.0);
    }
    assert_eq!(t.delivered(pipe, a), vec![1.0], "the tracer it took in");
    assert_eq!(t.delivered(pipe, b), vec![0.25], "and what followed");
    for _ in 0..25 {
        t.step(&[-2.0], &none, 10.0);
    }
    assert_eq!(
        t.delivered(pipe, a),
        vec![0.0],
        "then the clear water it held"
    );
    let parcels = t.parcels_of(pipe);
    assert!(parcels <= 3, "{parcels} parcels: no smearing");
}

/// Two streams, 3 kg/s of 80 and 1 kg/s of 20, meet at a junction: 4 kg/s of 65 leave.
#[test]
fn streams_mix_by_mass() {
    let mut t = Tracer::new(1);
    let hot = t.node(Holding::Source, 0.0, &[80.0]);
    let cold = t.node(Holding::Source, 0.0, &[20.0]);
    let joint = t.node(Holding::Mix, 0.0, &[0.0]);
    let out = t.node(Holding::Source, 0.0, &[0.0]);
    t.link(hot, joint, 0.0, &[0.0]);
    t.link(cold, joint, 0.0, &[0.0]);
    let pipe = t.link(joint, out, 100.0, &[0.0]);
    for _ in 0..100 {
        t.step(&[3.0, 1.0, 4.0], &[0.0; 4], 1.0);
    }
    assert!((t.content(joint)[0] - 65.0).abs() < 1e-12);
    assert!((t.delivered(pipe, out)[0] - 65.0).abs() < 1e-12);
}

/// A stirred tank of 1000 kg, 1 kg/s of tracer in and out: c = 1 − e^(−ṁt/M) (backward
/// Euler's error, first order, within 10⁻³ at Δt = M/ṁ/1000).
#[test]
fn a_stirred_tank_fills_exponentially() {
    let mut t = Tracer::new(1);
    let src = t.node(Holding::Source, 0.0, &[1.0]);
    let tank = t.node(Holding::Store, 1000.0, &[0.0]);
    let sink = t.node(Holding::Source, 0.0, &[0.0]);
    t.link(src, tank, 0.0, &[0.0]);
    t.link(tank, sink, 0.0, &[0.0]);
    for _ in 0..2000 {
        t.step(&[1.0, 1.0], &[0.0; 3], 1.0);
    }
    let exact = 1.0 - det::exp(-2.0);
    assert!(
        (t.content(tank)[0] / exact - 1.0).abs() < 1e-3,
        "{} against {exact}",
        t.content(tank)[0]
    );
    assert!((t.mass(tank) - 1000.0).abs() < 1e-9);
}

/// A network of every kind of node and link, its flows turning about: every quantity's books
/// close to rounding.
#[test]
fn every_quantity_is_booked() {
    let mut t = Tracer::new(2);
    let src = t.node(Holding::Source, 0.0, &[1.0, 300.0]);
    let joint = t.node(Holding::Mix, 0.0, &[0.0, 0.0]);
    let tank = t.node(Holding::Store, 50.0, &[0.2, 280.0]);
    let tap = t.node(Holding::Mix, 0.0, &[0.0, 0.0]);
    let drain = t.node(Holding::Source, 0.0, &[0.0, 290.0]);
    t.link(src, joint, 0.0, &[0.0, 0.0]);
    t.link(joint, tank, 30.0, &[0.5, 310.0]);
    t.link(tank, tap, 7.0, &[0.1, 290.0]);
    t.link(joint, tap, 13.0, &[0.0, 295.0]);
    t.link(tap, drain, 0.0, &[0.0, 0.0]);
    for k in 0..400 {
        let s = det::sin(f64::from(k) * 0.05);
        t.set(src, &[0.5 + 0.5 * s, 300.0 + 20.0 * s]);
        // In 4 at the joint, x to the tank (x − 1 to the tap from it, 1 kg/s drawn off at the
        // tap), the rest to the tap; the tank gains 1 kg/s.
        let x = 2.0 + 1.5 * s;
        let flows = [4.0, x, x - 1.0, 4.0 - x, 2.0];
        t.step(&flows, &[0.0, 0.0, 0.0, 1.0, 0.0], 1.0);
    }
    let held = t.held();
    for (q, h) in held.iter().enumerate() {
        let off = t.ledger(q).relative_imbalance(*h);
        assert!(off < 1e-12, "quantity {q}: {off}");
    }
    assert!((t.mass(tank) - 450.0).abs() < 1e-9, "{}", t.mass(tank));
}
