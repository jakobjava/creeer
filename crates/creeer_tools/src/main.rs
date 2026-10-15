//! Developer tools: data validator, headless scenario runner, benchmarks.
//!
//! `creeer_tools bench-spikes [--metrics <file>]` measures the F0 spikes against their
//! budgets and optionally writes the metrics for `cargo xtask perf`.

mod spikes;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: creeer_tools <command>

commands:
  bench-spikes [--metrics <file>]   measure the F0 spikes against their budgets";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bench-spikes") => {
            let metrics = match args.get(1).map(String::as_str) {
                None => None,
                Some("--metrics") => match args.get(2) {
                    Some(path) => Some(PathBuf::from(path)),
                    None => return usage(),
                },
                Some(_) => return usage(),
            };
            spikes::run(metrics.as_deref())
        }
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::FAILURE
}
