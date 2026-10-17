//! F6.7 logic: 10,000 gates (a hundred machines' controls of a hundred blocks each: sensors,
//! gates, latches, timers, comparators, PID blocks, a feedback loop) and 100 Lua
//! microcontrollers running about 1,000 instructions each a tick.

use crate::spikes::{Better, MIN_SPEC_CORE_FACTOR, Report, p, time_ticks};
use creeer_logic::{BlockId, Compare, Kind, Limits, Network, Pid, Script, Signal};

/// Budget on the minimum-spec CPU (docs/f6-plan.md).
const BUDGET_MS: f64 = 0.5;

/// A controller's script: a filter and a small optimisation, about 1,000 instructions a tick.
const SCRIPT: &str = "memory.acc = memory.acc or 0
function tick(dt)
  local x, best, acc = input[1], 0, memory.acc
  for i = 1, 100 do
    acc = acc * 0.99 + x * i
    if acc > best then best = acc end
  end
  memory.acc = acc * 1e-3
  output[1] = best
  output[2] = best > 50 and x > 0
end";

/// One machine's controls: 100 blocks; its sensor is returned.
fn machine(net: &mut Network) -> BlockId {
    let sensor = net.add(Kind::Sensor);
    let zero = net.add(Kind::Constant(Signal::Number(0.0)));
    let above = net.add(Kind::Compare(Compare::Greater));
    net.wire((sensor, 0), above, 0);
    net.wire((zero, 0), above, 1);
    let pid = net.add(Kind::Pid(Pid {
        kp: 1.0,
        ki: 0.5,
        kd: 0.01,
        min: -1.0,
        max: 1.0,
    }));
    net.wire((zero, 0), pid, 0);
    net.wire((sensor, 0), pid, 1);
    let latch = net.add(Kind::SrLatch);
    net.wire((above, 0), latch, 0);
    let timer = net.add(Kind::OnDelay(0.2));
    net.wire((latch, 0), timer, 0);
    net.wire((timer, 0), latch, 1);
    // A chain of gates mixing it all, its end fed back to its start (a one-tick register).
    let mut last = timer;
    let kinds = [
        Kind::And,
        Kind::Or,
        Kind::Xor,
        Kind::Not,
        Kind::Nand,
        Kind::Nor,
    ];
    let first = net.len();
    for k in 0..94 {
        let g = net.add(kinds[k % kinds.len()]);
        net.wire((last, 0), g, 0);
        net.wire((if k % 2 == 0 { above } else { latch }, 0), g, 1);
        last = g;
    }
    net.wire((last, 0), BlockId(first), 2);
    sensor
}

/// Runs the logic benchmark.
pub fn run(report: &mut Report) {
    println!("F6.7 logic: 10,000 gates and 100 scripts of ~1,000 instructions (ms, p50 / p99)");
    let mut net = Network::new();
    let sensors: Vec<BlockId> = (0..100).map(|_| machine(&mut net)).collect();
    let limits = Limits {
        instructions: 1_200,
        ..Limits::default()
    };
    let scripts: Vec<BlockId> = sensors
        .iter()
        .enumerate()
        .map(|(k, &s)| {
            let b = net.add_script(Script::new(SCRIPT, 1, 2, limits, k as u64));
            net.wire((s, 0), b, 0);
            b
        })
        .collect();
    let gates = net.len() - scripts.len();
    let mut tick = 0u32;
    let mut step = || {
        for (k, &s) in sensors.iter().enumerate() {
            let phase = f64::from(tick) / 60.0 + k as f64;
            net.set_sensor(s, creeer_math::det::sin(phase) * 10.0);
        }
        net.tick(1.0 / 60.0);
        tick += 1;
    };
    for _ in 0..120 {
        step();
    }
    let t = time_ticks(step, 600);
    let stopped = scripts
        .iter()
        .filter(|&&b| net.script(b).and_then(Script::halted).is_some())
        .count();
    println!(
        "  {gates} gates, {} scripts: {:.3} / {:.3} | scripts stopped: {stopped}",
        scripts.len(),
        p(&t, 50.0),
        p(&t, 99.0)
    );
    report.metric(
        "logic_10k_gates_100_scripts_p50_ms",
        p(&t, 50.0),
        Better::Lower,
    );
    let projected = p(&t, 50.0) / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= BUDGET_MS && stopped == 0,
        format!(
            "10,000 gates and 100 scripts projected to min-spec: {projected:.2} ms <= {BUDGET_MS} ms"
        ),
    );
    println!();
}
