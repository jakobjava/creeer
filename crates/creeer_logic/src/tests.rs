//! Gates by their truth tables, latches and flip-flops by their sequences, timers by when they
//! switch (at a tick of 1/8 s, exact in binary), feedback through its one-tick register, a PID
//! block holding a plant, and a network saved and loaded going on as it would have.

use super::*;

const DT: f64 = 0.125;

/// A network of `kind` with two sensors on its inputs and an actuator on its output.
fn two_in(kind: Kind) -> (Network, BlockId, BlockId, BlockId) {
    let mut net = Network::new();
    let (a, b) = (net.add(Kind::Sensor), net.add(Kind::Sensor));
    let g = net.add(kind);
    let out = net.add(Kind::Actuator);
    net.wire((a, 0), g, 0);
    net.wire((b, 0), g, 1);
    net.wire((g, 0), out, 0);
    (net, a, b, out)
}

#[test]
fn gates_follow_their_truth_tables() {
    let tables = [
        (Kind::And, [false, false, false, true]),
        (Kind::Or, [false, true, true, true]),
        (Kind::Xor, [false, true, true, false]),
        (Kind::Nand, [true, true, true, false]),
        (Kind::Nor, [true, false, false, false]),
    ];
    for (kind, expected) in tables {
        let (mut net, a, b, out) = two_in(kind);
        for (k, want) in expected.into_iter().enumerate() {
            net.set_sensor(a, k & 2 != 0);
            net.set_sensor(b, k & 1 != 0);
            net.tick(DT);
            assert_eq!(net.value((out, 0)), Signal::Bool(want), "{kind:?} row {k}");
        }
    }
    let mut net = Network::new();
    let (a, not) = (net.add(Kind::Sensor), net.add(Kind::Not));
    net.wire((a, 0), not, 0);
    for x in [false, true] {
        net.set_sensor(a, x);
        net.tick(DT);
        assert_eq!(net.value((not, 0)), Signal::Bool(!x));
    }
}

#[test]
fn latches_and_flip_flops_hold_what_they_were_given() {
    // Set, hold, reset, hold, both (reset wins).
    let (mut net, s, r, out) = two_in(Kind::SrLatch);
    let steps = [
        (1, 0, true),
        (0, 0, true),
        (0, 1, false),
        (0, 0, false),
        (1, 1, false),
    ];
    for (set, reset, want) in steps {
        net.set_sensor(s, set == 1);
        net.set_sensor(r, reset == 1);
        net.tick(DT);
        assert_eq!(net.value((out, 0)), Signal::Bool(want), "S {set} R {reset}");
    }
    // D flip-flop: takes the data on the clock's rising edge only.
    let (mut net, d, clock, out) = two_in(Kind::DFlipFlop);
    let steps = [
        (1.5, 0, 0.0),
        (1.5, 1, 1.5),
        (2.5, 1, 1.5),
        (2.5, 0, 1.5),
        (2.5, 1, 2.5),
    ];
    for (data, c, want) in steps {
        net.set_sensor(d, data);
        net.set_sensor(clock, c == 1);
        net.tick(DT);
        assert_eq!(net.value((out, 0)).number(), want, "D {data} C {c}");
    }
}

/// The ticks (from 1) on which a timer of `kind` is on, for an input on from tick 1 to 6.
fn timer(kind: Kind) -> Vec<usize> {
    let mut net = Network::new();
    let input = net.add(Kind::Sensor);
    let t = net.add(kind);
    net.wire((input, 0), t, 0);
    (1..=14)
        .filter(|&tick| {
            net.set_sensor(input, (1..=6).contains(&tick));
            net.tick(DT);
            net.value((t, 0)).truth()
        })
        .collect()
}

#[test]
fn timers_switch_on_time() {
    // On for 0.5 s first: the fourth tick of input (4 × 1/8 s).
    assert_eq!(timer(Kind::OnDelay(0.5)), vec![4, 5, 6]);
    // On with the input and 0.5 s after: off on the fourth tick without it.
    assert_eq!(timer(Kind::OffDelay(0.5)), vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
    // A pulse of 0.25 s from the rising edge.
    assert_eq!(timer(Kind::Pulse(0.25)), vec![1, 2]);
}

#[test]
fn comparators_and_a_schmitt_trigger() {
    let (mut net, a, b, out) = two_in(Kind::Compare(Compare::AtLeast));
    for (x, y, want) in [(1.0, 2.0, false), (2.0, 2.0, true), (3.0, 2.0, true)] {
        net.set_sensor(a, x);
        net.set_sensor(b, y);
        net.tick(DT);
        assert_eq!(net.value((out, 0)), Signal::Bool(want));
    }
    let mut net = Network::new();
    let x = net.add(Kind::Sensor);
    let s = net.add(Kind::Schmitt {
        low: 1.0,
        high: 2.0,
    });
    net.wire((x, 0), s, 0);
    let on: Vec<bool> = [0.5, 1.5, 2.5, 1.5, 0.5, 1.5]
        .into_iter()
        .map(|v| {
            net.set_sensor(x, v);
            net.tick(DT);
            net.value((s, 0)).truth()
        })
        .collect();
    assert_eq!(on, [false, false, true, true, false, false]);
}

/// A loop passes through a one-tick register: a gate inverting itself toggles every tick, and
/// so does the first of three in a ring (each passes its change on within the tick, the first
/// reads the last's from the tick before: an odd ring inverts once a tick).
#[test]
fn feedback_passes_through_a_one_tick_register() {
    let mut net = Network::new();
    let not = net.add(Kind::Not);
    net.wire((not, 0), not, 0);
    let seen: Vec<bool> = (0..4)
        .map(|_| {
            net.tick(DT);
            net.value((not, 0)).truth()
        })
        .collect();
    assert_eq!(seen, [true, false, true, false]);
    let mut net = Network::new();
    let ring: Vec<BlockId> = (0..3).map(|_| net.add(Kind::Not)).collect();
    for k in 0..3 {
        net.wire((ring[k], 0), ring[(k + 1) % 3], 0);
    }
    let first: Vec<bool> = (0..6)
        .map(|_| {
            net.tick(DT);
            net.value((ring[0], 0)).truth()
        })
        .collect();
    assert_eq!(first, [true, false, true, false, true, false]);
}

/// A PID block brings a first-order plant (time constant 2 s, gain 1) to its setpoint and
/// holds it, its integral not winding up while its output was saturated.
#[test]
fn a_pid_block_holds_a_plant() {
    let mut net = Network::new();
    let (setpoint, measured) = (
        net.add(Kind::Constant(Signal::Number(3.0))),
        net.add(Kind::Sensor),
    );
    let pid = net.add(Kind::Pid(Pid {
        kp: 2.0,
        ki: 1.0,
        kd: 0.0,
        min: 0.0,
        max: 5.0,
    }));
    net.wire((setpoint, 0), pid, 0);
    net.wire((measured, 0), pid, 1);
    let (mut y, dt) = (0.0, 1.0 / 60.0);
    let mut overshoot: f64 = 0.0;
    for _ in 0..1200 {
        net.set_sensor(measured, y);
        net.tick(dt);
        let u = net.value((pid, 0)).number();
        y += dt * (u - y) / 2.0;
        overshoot = overshoot.max(y - 3.0);
    }
    assert!((y - 3.0).abs() < 1e-3, "settles at {y}");
    assert!(overshoot < 0.3, "overshoot {overshoot}");
}

/// A network saved and loaded goes on as the original does, bit for bit: gates, timers, a
/// PID block and a script with memory and random numbers.
#[test]
fn a_saved_network_goes_on_as_it_would_have() {
    let source = "memory.n = memory.n or 0\n\
                  function tick(dt)\n  memory.n = memory.n + 1\n\
                  output[1] = input[1] * memory.n + math.random()\nend";
    let mut net = Network::new();
    let clock = net.add(Kind::Pulse(0.25));
    let not = net.add(Kind::Not);
    net.wire((not, 0), clock, 0);
    net.wire((clock, 0), not, 0);
    let script = net.add_script(Script::new(source, 1, 1, Limits::default(), 7));
    net.wire((clock, 0), script, 0);
    for _ in 0..10 {
        net.tick(DT);
    }
    let mut loaded = Network::from_bytes(&net.to_bytes()).expect("a network");
    assert_eq!(loaded.to_bytes(), net.to_bytes());
    for _ in 0..20 {
        net.tick(DT);
        loaded.tick(DT);
        assert_eq!(loaded.value((script, 0)), net.value((script, 0)));
        assert_eq!(loaded.value((clock, 0)), net.value((clock, 0)));
    }
}

#[test]
#[ignore]
fn tmp_timing() {
    let script = "memory.acc = memory.acc or 0\nfunction tick(dt)\n  local x, best, acc = input[1], 0, memory.acc\n  for i = 1, 100 do\n    acc = acc * 0.99 + x * i\n    if acc > best then best = acc end\n  end\n  memory.acc = acc * 1e-3\n  output[1] = best\n  output[2] = best > 50 and x > 0\nend";
    // Gates alone.
    let mut net = Network::new();
    let s = net.add(Kind::Sensor);
    let mut last = s;
    for k in 0..10_000 {
        let g = net.add([Kind::And, Kind::Or, Kind::Not, Kind::Xor][k % 4]);
        net.wire((last, 0), g, 0);
        net.wire((s, 0), g, 1);
        last = g;
    }
    for _ in 0..50 {
        net.tick(DT);
    }
    let t = std::time::Instant::now();
    for k in 0..200 {
        net.set_sensor(s, k % 3 == 0);
        net.tick(DT);
    }
    eprintln!("GATES {:.3} ms", t.elapsed().as_secs_f64() * 1e3 / 200.0);
    // One script, sequential.
    let mut sc = Script::new(script, 1, 2, Limits::default(), 1);
    let t = std::time::Instant::now();
    for k in 0..2000 {
        sc.run(&[Signal::Number(f64::from(k % 7))], DT);
    }
    eprintln!(
        "SCRIPT {:.4} ms each",
        t.elapsed().as_secs_f64() * 1e3 / 2000.0
    );
    let mut sc = Script::new(
        "function tick(dt) output[1] = input[1] end",
        1,
        1,
        Limits::default(),
        1,
    );
    let t = std::time::Instant::now();
    for k in 0..20000 {
        sc.run(&[Signal::Number(f64::from(k % 7))], DT);
    }
    eprintln!(
        "EMPTY SCRIPT {:.5} ms each",
        t.elapsed().as_secs_f64() * 1e3 / 20000.0
    );
}

#[test]
#[ignore]
fn tmp_timing2() {
    for kind in [Kind::Not, Kind::And, Kind::Constant(Signal::Bool(true))] {
        let mut net = Network::new();
        let s = net.add(Kind::Sensor);
        let mut last = s;
        for _ in 0..10_000 {
            let g = net.add(kind);
            net.wire((last, 0), g, 0);
            last = g;
        }
        for _ in 0..50 {
            net.tick(DT);
        }
        let t = std::time::Instant::now();
        for k in 0..500 {
            net.set_sensor(s, k % 3 == 0);
            net.tick(DT);
        }
        eprintln!("{kind:?} {:.3} ms", t.elapsed().as_secs_f64() * 1e3 / 500.0);
    }
}
