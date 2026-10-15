//! Project automation, run through the `cargo xtask` alias (see `.cargo/config.toml`).
//!
//! - `cargo xtask ci`: formatting check, clippy with warnings as errors, all tests.
//! - `cargo xtask perf [--save-baseline]`: runs the performance spikes in release mode,
//!   checks their budgets and compares against `perf/baseline.txt` (fails on a regression of
//!   more than 10 %, plan.md §5).

use std::collections::BTreeMap;
use std::env;
use std::path::Path;
use std::process::{Command, ExitCode};

const USAGE: &str = "usage: cargo xtask <task>

tasks:
  ci                        fmt check, clippy (-D warnings), tests
  perf [--save-baseline]    spike budgets + regression check against perf/baseline.txt";

/// Allowed slowdown before `perf` reports a regression.
const REGRESSION_TOLERANCE: f64 = 0.10;
const LATEST: &str = "target/perf/latest.txt";
const BASELINE: &str = "perf/baseline.txt";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["ci"] => ci(),
        ["perf"] => perf(false),
        ["perf", "--save-baseline"] => perf(true),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn ci() -> ExitCode {
    let steps: &[(&str, &[&str])] = &[
        ("format", &["fmt", "--all", "--check"]),
        (
            "clippy",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
        ),
        ("tests", &["test", "--workspace"]),
    ];
    for (name, args) in steps {
        println!("==> {name}: cargo {}", args.join(" "));
        if !cargo(args) {
            eprintln!("xtask ci: step `{name}` failed");
            return ExitCode::FAILURE;
        }
    }
    println!("xtask ci: all steps passed");
    ExitCode::SUCCESS
}

fn perf(save_baseline: bool) -> ExitCode {
    println!("==> running spikes (release build)");
    let budgets_ok = cargo(&[
        "run",
        "--quiet",
        "--release",
        "--package",
        "creeer_tools",
        "--",
        "bench-spikes",
        "--metrics",
        LATEST,
    ]);
    let Some(latest) = read_metrics(Path::new(LATEST)) else {
        eprintln!("xtask perf: no metrics were written to {LATEST}");
        return ExitCode::FAILURE;
    };

    if save_baseline {
        let mut text = String::from(
            "# Performance baseline for `cargo xtask perf` (machine-specific).\n\
             # Regenerate with `cargo xtask perf --save-baseline` after an intended change.\n",
        );
        text.push_str(&std::fs::read_to_string(LATEST).unwrap_or_default());
        if let Err(e) =
            std::fs::create_dir_all("perf").and_then(|()| std::fs::write(BASELINE, text))
        {
            eprintln!("xtask perf: could not write {BASELINE}: {e}");
            return ExitCode::FAILURE;
        }
        println!("xtask perf: baseline saved to {BASELINE}");
        return if budgets_ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }

    let regressions = match read_metrics(Path::new(BASELINE)) {
        Some(baseline) => compare(&baseline, &latest),
        None => {
            println!("xtask perf: no {BASELINE} yet; run with --save-baseline to create one");
            0
        }
    };
    if budgets_ok && regressions == 0 {
        println!("xtask perf: budgets met, no regressions");
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "xtask perf: {}{regressions} regression(s)",
            if budgets_ok { "" } else { "budget(s) missed; " }
        );
        ExitCode::FAILURE
    }
}

/// Metric name → (value, direction: "lower", "higher" or "info").
type Metrics = BTreeMap<String, (f64, String)>;

fn read_metrics(path: &Path) -> Option<Metrics> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut metrics = Metrics::new();
    for line in text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(value), Some(better)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if let Ok(value) = value.parse::<f64>() {
            metrics.insert(name.to_owned(), (value, better.to_owned()));
        }
    }
    Some(metrics)
}

/// Prints a comparison table and returns the number of regressions.
fn compare(baseline: &Metrics, latest: &Metrics) -> usize {
    println!(
        "\n{:<36} {:>12} {:>12} {:>8}",
        "metric", "baseline", "latest", "change"
    );
    let mut regressions = 0;
    for (name, (value, direction)) in latest {
        let value = *value;
        let Some(&(base, _)) = baseline.get(name) else {
            println!("{name:<36} {:>12} {value:>12.4} {:>8}", "-", "new");
            continue;
        };
        let change = if base == 0.0 { 0.0 } else { value / base - 1.0 };
        let regressed = match direction.as_str() {
            "lower" => change > REGRESSION_TOLERANCE,
            "higher" => change < -REGRESSION_TOLERANCE,
            _ => false, // informational
        };
        regressions += usize::from(regressed);
        println!(
            "{name:<36} {base:>12.4} {value:>12.4} {:>+7.1}%{}",
            change * 100.0,
            if regressed {
                "  REGRESSION"
            } else if direction == "info" {
                "  (info)"
            } else {
                ""
            }
        );
    }
    println!();
    regressions
}

/// Runs the same cargo binary that invoked us, so the pinned toolchain is used.
fn cargo(args: &[&str]) -> bool {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    Command::new(cargo)
        .args(args)
        .status()
        .is_ok_and(|status| status.success())
}
