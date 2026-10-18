//! `creeer_tools stress [--short] [--check | --save-golden] [--metrics <file>] [--only <name>]`
//! (F7.5, plan.md §5): the stress scenarios, headless, each with its tick percentiles (p50,
//! p99, worst; the first tick, which takes the setup, apart) and a hash of its state at the
//! end. `STRESS_TRACE=1` lists the ticks over 10 ms (with the physics phases when built with
//! `--features creeer_physics/profile`).
//!
//! - `--short`: the lengths CI runs (on every OS, so the hashes compare platforms at scale);
//! - `--check`: fails if a hash differs from `tests/scenarios/stress.txt`;
//! - `--save-golden`: writes the hashes there;
//! - `--metrics`: appends p50s, worst and first ticks for `cargo xtask perf`;
//! - `--profile`: where the first tick and the worst went (`creeer_core::profile`);
//! - `--gate`: fails if a worst or first tick (or a scenario's own gauge, the flyover's longest
//!   wait for detail) is over its ceiling (full lengths only).

use crate::stress_flyover::flyover;
use crate::stress_scenes::{Scene, beams, cars, factory};
use crate::stress_world::server;
use creeer_core::{TimingStats, profile};
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

const GOLDEN: &str = "tests/scenarios/stress.txt";

/// A paced scenario's frame: 60 a second.
const FRAME: std::time::Duration = std::time::Duration::from_micros(16_667);

type Build = fn(u32) -> Scene;

/// A scenario: its name, what it is, how to build it, its ticks (full, short), and the
/// ceilings of its worst tick and of its first (ms, on the development machine at full
/// length, gated by `--gate`: about a quarter above what they measured, 2026-10-18).
struct Scenario {
    name: &'static str,
    what: &'static str,
    build: Build,
    ticks: (u32, u32),
    worst: f64,
    first: f64,
}

const SCENARIOS: [Scenario; 5] = [
    Scenario {
        name: "beams",
        what: "50,000 beams under moving loads",
        build: |_| beams(),
        ticks: (600, 60),
        worst: 27.0,
        first: 26.0,
    },
    Scenario {
        name: "cars",
        what: "10 cars crashing",
        build: |_| cars(),
        ticks: (300, 120),
        worst: 180.0,
        first: 2.0,
    },
    Scenario {
        name: "factory",
        what: "every domain's reference load and 100 powerhouses",
        build: |_| factory(),
        ticks: (600, 60),
        worst: 8.0,
        first: 50.0,
    },
    Scenario {
        name: "server",
        what: "8 players, 100,000 parked, 10 abstract machines",
        build: |_| server(),
        ticks: (1200, 120),
        worst: 8.5,
        first: 35.0,
    },
    Scenario {
        name: "flyover",
        what: "from orbit to the ground, the terrain streamed",
        build: flyover,
        ticks: (1200, 240),
        worst: 2.0,
        first: 1.0,
    },
];

fn p(stats: &TimingStats, q: f64) -> f64 {
    stats.percentile_ms(q).unwrap_or(f64::NAN)
}

pub fn run(args: &[String]) -> ExitCode {
    let flag = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| args.windows(2).find(|w| w[0] == f).map(|w| w[1].clone());
    let (short, check, save) = (flag("--short"), flag("--check"), flag("--save-golden"));
    // Ceilings are for the full lengths.
    let gate = flag("--gate") && !short;
    let only = value("--only");
    // Ticks over 10 ms, with the physics phases when built with `creeer_physics/profile`.
    let trace = std::env::var_os("STRESS_TRACE").is_some();
    let golden = std::fs::read_to_string(GOLDEN).unwrap_or_default();
    let (mut lines, mut metrics, mut failed) = (String::new(), String::new(), false);
    for Scenario {
        name,
        what,
        build,
        ticks: (full, brief),
        worst,
        first: first_ceiling,
    } in SCENARIOS
    {
        if only.as_deref().is_some_and(|o| o != name) {
            continue;
        }
        let ticks = if short { brief } else { full };
        let setup = Instant::now();
        let mut scene = build(ticks);
        let built = setup.elapsed().as_secs_f64();
        // The first tick apart: it takes the scenario's setup (first factorisations, a load's
        // first regions), the others are what it costs to keep running.
        let (mut stats, mut first) = (TimingStats::new(ticks as usize), 0.0);
        // `--profile`: every tick cut into a frame (the tool's clock before any other).
        let wanted = flag("--profile").then(profile::want);
        let mut clock = wanted.as_ref().and_then(|_| profile::clock(3));
        let mut seen = 0;
        let _ = profile::frames_since(&mut seen);
        for tick in 0..ticks {
            let start = Instant::now();
            (scene.tick)(tick);
            let took = start.elapsed();
            if let Some(c) = &mut clock {
                c.end_tick(u64::from(tick));
            }
            if tick == 0 {
                first = took.as_secs_f64() * 1e3;
            } else {
                stats.push(took);
            }
            let spent = creeer_physics::profile::take();
            if trace && took.as_secs_f64() > 0.01 {
                let phases: Vec<String> = creeer_physics::profile::NAMES
                    .iter()
                    .filter(|(s, _)| spent[*s as usize] > 0)
                    .map(|(s, n)| format!("{n} {}", spent[*s as usize]))
                    .collect();
                eprintln!(
                    "tick {tick}: {:.3} ms; {}",
                    took.as_secs_f64() * 1e3,
                    phases.join(", ")
                );
            }
            if scene.paced {
                std::thread::sleep(FRAME.saturating_sub(took));
            }
        }
        let hash = (scene.hash)();
        println!(
            "{name:8} {what:52} {ticks:5} ticks: {first:7.3} first {:7.3} p50 {:7.3} p99 {:7.3} worst ms | hash {hash:016x} (built in {built:.1} s)",
            p(&stats, 50.0),
            p(&stats, 99.0),
            p(&stats, 100.0)
        );
        let notes = (scene.notes)();
        if !notes.text.is_empty() {
            println!("         {}", notes.text);
        }
        if clock.take().is_some() {
            print_profile(&profile::frames_since(&mut seen));
        }
        drop(wanted);
        let mut gauges = vec![
            ("worst_ms", p(&stats, 100.0), worst),
            ("first_ms", first, first_ceiling),
        ];
        gauges.extend(notes.gauges);
        for (gauge, value, ceiling) in gauges {
            let _ = writeln!(metrics, "stress_{name}_{gauge} {value:.6} info");
            if gate && value > ceiling {
                println!("  {gauge} {value:.3} over its ceiling {ceiling}");
                failed = true;
            }
        }
        let _ = writeln!(lines, "{name} {ticks} {hash:016x}");
        let _ = writeln!(metrics, "stress_{name}_p50_ms {:.6} lower", p(&stats, 50.0));
        if check {
            let want = golden
                .lines()
                .find(|l| l.starts_with(&format!("{name} {ticks} ")))
                .and_then(|l| l.split_whitespace().nth(2));
            match want {
                Some(w) if w == format!("{hash:016x}") => {}
                Some(w) => {
                    println!("  hash differs from the golden {w}");
                    failed = true;
                }
                None => println!("  no golden hash for {name} at {ticks} ticks"),
            }
        }
    }
    if save {
        let mut text = golden
            .lines()
            .filter(|l| {
                !lines
                    .lines()
                    .any(|n| n.split(' ').take(2).eq(l.split(' ').take(2)))
            })
            .map(|l| format!("{l}\n"))
            .collect::<String>();
        text.push_str(&lines);
        if let Err(e) = std::fs::write(GOLDEN, text) {
            eprintln!("could not write {GOLDEN}: {e}");
            return ExitCode::FAILURE;
        }
        println!("hashes saved to {GOLDEN}");
    }
    if let Some(path) = value("--metrics") {
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        text.push_str(&metrics);
        let _ = std::fs::create_dir_all(Path::new(&path).parent().unwrap_or(Path::new(".")));
        let _ = std::fs::write(&path, text);
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Where the first tick and the worst of the others went (`--profile`).
fn print_profile(frames: &[profile::Frame]) {
    let worst = frames.iter().skip(1).max_by_key(|f| f.duration);
    for (what, frame) in [("first", frames.first()), ("worst", worst)] {
        let Some(frame) = frame else {
            continue;
        };
        let parts: Vec<String> = frame
            .totals()
            .iter()
            .take(10)
            .map(|(name, ns)| format!("{name} {:.2}", *ns as f64 / 1e6))
            .collect();
        println!(
            "         {what} tick ({}, {:.2} ms): {}",
            frame.tick,
            frame.duration as f64 / 1e6,
            parts.join(", ")
        );
    }
}
