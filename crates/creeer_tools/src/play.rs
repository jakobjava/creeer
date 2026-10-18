//! `creeer_tools play [--short] [--check | --save-golden] [--gate] [--metrics <file>]
//! [--only <name>] [--record <file>] [--replay <file>]` (G1.0, `docs/g1-plan.md`): the play
//! scenes (`creeer_server::play::scenes`), headless over the real server path, each with its
//! tick percentiles, its most work in a tick, its bandwidth and a hash of its end state.
//! `PLAY_TRACE=<ms>` lists the ticks over that (2 ms if not a number).
//!
//! - `--short`: CI's lengths (on every OS: the hashes compare platforms);
//! - `--check` / `--save-golden`: the hashes against / into `tests/scenarios/play.txt`;
//! - `--gate`: fails if a worst tick is over 5 ms or a p99 over 2 ms (plan.md, Phase B; the
//!   development machine, full lengths only);
//! - `--metrics`: appends p50s and worst ticks for `cargo xtask perf`;
//! - `--record <file>`: writes the (one) scene's recording; `--replay <file>`: plays a
//!   recording (a live client's, or one written by `--record`) in its scene's world.

use creeer_server::play::record::Recording;
use creeer_server::play::scenes::{SCENES, Scene, run_with, scene, terrain};
use creeer_server::play::{Report, replay};
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

const GOLDEN: &str = "tests/scenarios/play.txt";
/// Phase B's gates on the development machine (ms).
const WORST: f64 = 5.0;
const P99: f64 = 2.0;

fn print(name: &str, what: &str, report: &Report) {
    let (tick, work) = report.most_work();
    let (down, up) = report.kbits();
    println!(
        "{name:10} {what:56} {:5} ticks: {:6.3} p50 {:6.3} p99 {:6.3} worst ms | work {work} (tick {tick}) | {down:4.0} / {up:4.1} kbit/s | hash {:016x}",
        report.ticks.len(),
        report.percentile_ms(50.0),
        report.percentile_ms(99.0),
        report.percentile_ms(100.0),
        report.hash
    );
}

fn play(s: &Scene, ticks: u32, trace: Option<f64>, profiled: bool) -> Report {
    let wanted = profiled.then(creeer_core::profile::want);
    let mut clock = wanted.as_ref().and_then(|_| creeer_core::profile::clock(3));
    let mut seen = 0;
    let _ = creeer_core::profile::frames_since(&mut seen);
    let report = run_with(s, ticks, |t, cost| {
        if let Some(c) = &mut clock {
            c.end_tick(u64::from(t));
        }
        if trace.is_some_and(|ms| cost.ms > ms) {
            eprintln!("tick {t}: {:.3} ms, {} iterations", cost.ms, cost.work);
        }
    });
    if clock.take().is_some() {
        print_profile(&creeer_core::profile::frames_since(&mut seen));
    }
    report
}

/// Where a typical tick and the worst went (`--profile`; a frame also holds the clients'
/// own work between server ticks).
fn print_profile(frames: &[creeer_core::profile::Frame]) {
    let mut sorted: Vec<_> = frames.iter().skip(1).collect();
    sorted.sort_by_key(|f| f.duration);
    for (what, frame) in [
        ("p50", sorted.get(sorted.len() / 2)),
        ("worst", sorted.last()),
    ] {
        let Some(frame) = frame else { continue };
        let parts: Vec<String> = (frame.totals().iter().take(12))
            .map(|(name, ns)| format!("{name} {:.2}", *ns as f64 / 1e6))
            .collect();
        println!(
            "  {what} frame ({:.2} ms): {}",
            frame.duration as f64 / 1e6,
            parts.join(", ")
        );
    }
}

pub fn run(args: &[String]) -> ExitCode {
    let flag = |f: &str| args.iter().any(|a| a == f);
    let value = |f: &str| args.windows(2).find(|w| w[0] == f).map(|w| w[1].clone());
    let (short, check, save) = (flag("--short"), flag("--check"), flag("--save-golden"));
    let gate = flag("--gate") && !short;
    let trace = std::env::var("PLAY_TRACE")
        .ok()
        .map(|ms| ms.parse::<f64>().unwrap_or(2.0));
    if let Some(path) = value("--replay") {
        let recording = match std::fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|b| Recording::from_bytes(&b).map_err(|e| e.to_string()))
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{path}: {e}");
                return ExitCode::FAILURE;
            }
        };
        let Some(s) = scene(&recording.scene) else {
            eprintln!("{path}: no play scene {}", recording.scene);
            return ExitCode::FAILURE;
        };
        let report = replay((s.build)(terrain()), &recording, &terrain());
        print(s.name, "(replayed)", &report);
        return ExitCode::SUCCESS;
    }
    let only = value("--only");
    let golden = std::fs::read_to_string(GOLDEN).unwrap_or_default();
    let (mut lines, mut metrics, mut failed) = (String::new(), String::new(), false);
    for s in SCENES
        .iter()
        .filter(|s| only.as_deref().is_none_or(|o| o == s.name))
    {
        let ticks = if short { s.ticks.1 } else { s.ticks.0 };
        let report = play(s, ticks, trace, flag("--profile"));
        print(s.name, s.what, &report);
        let (worst, p99) = (report.percentile_ms(100.0), report.percentile_ms(99.0));
        if gate {
            for (what, value, ceiling) in [("worst", worst, WORST), ("p99", p99, P99)] {
                if value > ceiling {
                    println!("  {what} tick {value:.3} ms over its gate {ceiling} ms");
                    failed = true;
                }
            }
        }
        let name = s.name;
        let _ = writeln!(
            metrics,
            "play_{name}_p50_ms {:.6} lower",
            report.percentile_ms(50.0)
        );
        let _ = writeln!(metrics, "play_{name}_worst_ms {worst:.6} info");
        let _ = writeln!(lines, "{name} {ticks} {:016x}", report.hash);
        if check {
            let want = (golden.lines())
                .find(|l| l.starts_with(&format!("{name} {ticks} ")))
                .and_then(|l| l.split_whitespace().nth(2));
            match want {
                Some(w) if w == format!("{:016x}", report.hash) => {}
                Some(w) => {
                    println!("  hash differs from the golden {w}");
                    failed = true;
                }
                None => println!("  no golden hash for {name} at {ticks} ticks"),
            }
        }
        if let Some(path) = value("--record") {
            if let Err(e) = std::fs::write(&path, report.recording.to_bytes()) {
                eprintln!("could not write {path}: {e}");
                return ExitCode::FAILURE;
            }
            println!("  recording written to {path}");
        }
    }
    if save {
        let mut text: String = (golden.lines())
            .filter(|l| !(lines.lines()).any(|n| n.split(' ').take(2).eq(l.split(' ').take(2))))
            .map(|l| format!("{l}\n"))
            .collect();
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
