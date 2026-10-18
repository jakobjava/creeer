//! `creeer_tools validate [--data <dir>] [--mods <dir>]`: every pack loaded and checked as
//! the game would merge them (`creeer_sim::content`), with the time it took.

use creeer_data::Location;
use std::process::ExitCode;
use std::time::Instant;

pub fn run(args: &[String]) -> ExitCode {
    let start = Instant::now();
    let location = Location::find(args);
    let data = match location.open() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    for pack in data.packs() {
        println!(
            "pack {} {} ({} files, {})",
            pack.name(),
            pack.manifest.version,
            pack.files.len(),
            pack.root.display()
        );
    }
    let report = creeer_sim::content::validate(&data);
    let counts: Vec<String> = report
        .counts
        .iter()
        .map(|(k, n)| format!("{n} {k}"))
        .collect();
    println!("checked {}", counts.join(", "));
    for w in &report.warnings {
        println!("warning: {w}");
    }
    for e in &report.errors {
        println!("error: {e}");
    }
    println!(
        "content {:016x}; {} errors, {} warnings in {:.0} ms",
        data.hash(),
        report.errors.len(),
        report.warnings.len(),
        start.elapsed().as_secs_f64() * 1e3
    );
    if report.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
