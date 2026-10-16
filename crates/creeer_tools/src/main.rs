//! Developer tools: data validator, headless scenario runner, benchmarks.
//!
//! `creeer_tools bench-spikes [--metrics <file>] [--report-only]` measures the spikes against
//! their budgets and optionally writes the metrics for `cargo xtask perf`. `--report-only`
//! (used by CI) reports missed budgets as warnings instead of failing.

mod image_diff;
mod solvers;
mod spikes;
mod terrain_bench;
mod world_bench;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: creeer_tools <command>

commands:
  bench-spikes [--metrics <file>] [--report-only]   measure the spikes against their budgets
  image-diff <a.png> <b.png>        share of visibly changed pixels between two images
  image-crop <in> <out> <x> <y> <w> <h> <zoom>   cut out and enlarge a region";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("image-crop") if args.len() == 8 => {
            let numbers: Vec<u32> = args[3..].iter().filter_map(|n| n.parse().ok()).collect();
            match numbers.as_slice() {
                &[x, y, w, h, zoom] => {
                    image_diff::crop(args[1].as_ref(), args[2].as_ref(), [x, y, w, h], zoom)
                }
                _ => usage(),
            }
        }
        Some("image-diff") => match (args.get(1), args.get(2)) {
            (Some(a), Some(b)) => image_diff::run(a.as_ref(), b.as_ref()),
            _ => usage(),
        },
        Some("bench-spikes") => {
            let (mut metrics, mut report_only) = (None, false);
            let mut rest = args[1..].iter();
            while let Some(arg) = rest.next() {
                match arg.as_str() {
                    "--metrics" => match rest.next() {
                        Some(path) => metrics = Some(PathBuf::from(path)),
                        None => return usage(),
                    },
                    "--report-only" => report_only = true,
                    _ => return usage(),
                }
            }
            spikes::run(metrics.as_deref(), report_only)
        }
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::FAILURE
}
