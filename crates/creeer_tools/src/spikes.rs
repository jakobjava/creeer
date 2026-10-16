//! F0 spike measurements (plan.md §7, F0 exit criteria).
//!
//! Prints a human-readable report and writes the key metrics as `name value` lines (for
//! `cargo xtask perf` to compare against a stored baseline). Exits with failure if a budget
//! is missed.

use creeer_core::TimingStats;
use creeer_physics::spike::{
    BridgeSpec, GRAVITY, Truss, TrussWorld, analytic_midspan_sag, linear_static_sag,
    measure_static_sag,
};
use creeer_terrain::spike::{CHUNK_CELLS, PlanetDensity, surface_chunk_origins};
use rayon::prelude::*;
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

pub const DT: f64 = 1.0 / 60.0;
/// Rough per-core speed of the minimum-spec CPU (Ryzen 5 2600 class) relative to a modern
/// desktop core. Used to project measurements made on faster machines.
pub const MIN_SPEC_CORE_FACTOR: f64 = 0.5;
/// Spike (a) budget: ms per tick for 5,000 beams on the minimum-spec CPU.
const XPBD_BUDGET_MS: f64 = 4.0;
/// Spike (b) budget: chunks per second on 6 minimum-spec cores.
const MESHING_TARGET_CHUNKS_PER_S: f64 = 200.0;

/// Whether a larger value of a metric is better or worse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Better {
    Lower,
    Higher,
    /// Reported but not gated: too noisy for a 10 % regression check.
    Info,
}

#[derive(Debug, Default)]
pub struct Report {
    metrics: Vec<(&'static str, f64, Better)>,
    failures: Vec<String>,
}

impl Report {
    pub fn metric(&mut self, name: &'static str, value: f64, better: Better) {
        self.metrics.push((name, value, better));
    }

    pub fn budget(&mut self, ok: bool, description: String) {
        println!(
            "  budget: {description} -> {}",
            if ok { "OK" } else { "MISSED" }
        );
        if !ok {
            self.failures.push(description);
        }
    }
}

/// Runs all spikes. `metrics_out`, if given, receives one `name value better` line per metric.
/// With `report_only` (CI on shared machines), missed budgets become GitHub Actions warnings
/// and the exit code stays successful; the budgets themselves are unchanged.
pub fn run(metrics_out: Option<&Path>, report_only: bool) -> ExitCode {
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    println!("Creëer F0 spikes  ({threads} hardware threads)\n");
    let mut report = Report::default();
    spike_a_speed(&mut report);
    spike_a_accuracy(&mut report);
    spike_b_meshing(&mut report, threads);
    crate::solvers::run(&mut report);
    crate::solvers::run_structures(&mut report);
    crate::world_bench::run(&mut report);
    crate::terrain_bench::run(&mut report, threads);
    crate::orbit_bench::run(&mut report);

    if let Some(path) = metrics_out {
        let mut text = String::new();
        for (name, value, better) in &report.metrics {
            let better = match better {
                Better::Lower => "lower",
                Better::Higher => "higher",
                Better::Info => "info",
            };
            let _ = writeln!(text, "{name} {value:.6} {better}");
        }
        if let Err(e) = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(path, text))
        {
            eprintln!("could not write metrics to {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    }

    if report.failures.is_empty() {
        println!("All spike budgets met.");
        ExitCode::SUCCESS
    } else if report_only {
        for failure in &report.failures {
            println!("::warning title=Missed spike budget::{failure}");
        }
        ExitCode::SUCCESS
    } else {
        println!("Missed budgets:\n  {}", report.failures.join("\n  "));
        ExitCode::FAILURE
    }
}

/// Times `ticks` steps of `step`, after a short warm-up.
pub fn time_ticks(mut step: impl FnMut(), ticks: usize) -> TimingStats {
    for _ in 0..20 {
        step();
    }
    let mut stats = TimingStats::new(ticks);
    for _ in 0..ticks {
        let start = Instant::now();
        step();
        stats.push(start.elapsed());
    }
    stats
}

pub fn p(stats: &TimingStats, percentile: f64) -> f64 {
    stats.percentile_ms(percentile).unwrap_or(f64::NAN)
}

fn spike_a_speed(report: &mut Report) {
    let world = TrussWorld::bridges(16);
    println!(
        "Spike (a) XPBD speed: {} beams, {} nodes in {} islands",
        world.beam_count(),
        world.trusses.iter().map(Truss::node_count).sum::<usize>(),
        world.trusses.len()
    );
    println!("  substeps | sequential p50 / p99 (ms) | parallel p50 / p99 (ms)");
    let mut seq_8 = f64::NAN;
    for (substeps, seq_name, par_name) in [
        (
            8,
            "xpbd_5k_beams_8sub_seq_p50_ms",
            "xpbd_5k_beams_8sub_par_p50_ms",
        ),
        (
            20,
            "xpbd_5k_beams_20sub_seq_p50_ms",
            "xpbd_5k_beams_20sub_par_p50_ms",
        ),
        (
            50,
            "xpbd_5k_beams_50sub_seq_p50_ms",
            "xpbd_5k_beams_50sub_par_p50_ms",
        ),
    ] {
        let mut seq = world.clone();
        let s = time_ticks(|| seq.step_sequential(DT, substeps), 300);
        let mut par = world.clone();
        let q = time_ticks(|| par.step_parallel(DT, substeps), 300);
        println!(
            "  {substeps:>8} | {:>10.3} / {:<12.3} | {:>8.3} / {:.3}",
            p(&s, 50.0),
            p(&s, 99.0),
            p(&q, 50.0),
            p(&q, 99.0),
        );
        report.metric(seq_name, p(&s, 50.0), Better::Lower);
        // Sub-millisecond work spread over many threads mostly measures thread wake-up
        // jitter, so parallel timings are informational only.
        report.metric(par_name, p(&q, 50.0), Better::Info);
        if substeps == 8 {
            seq_8 = p(&s, 50.0);
        }
    }
    // Single-island (sequential) cost is the honest one: a single large structure cannot use
    // island parallelism.
    let projected = seq_8 / MIN_SPEC_CORE_FACTOR;
    report.budget(
        projected <= XPBD_BUDGET_MS,
        format!(
            "XPBD 8 substeps, one core, projected to min-spec: {projected:.2} ms <= {XPBD_BUDGET_MS} ms"
        ),
    );
    println!();
}

fn spike_a_accuracy(report: &mut Report) {
    let spec = BridgeSpec::REFERENCE;
    let truss = Truss::box_bridge(Default::default(), spec);
    let mass = truss.total_mass(&spec);
    let analytic = analytic_midspan_sag(&spec, mass);
    let exact = linear_static_sag(spec);
    println!(
        "Spike (a) XPBD accuracy: {:.0} m steel box truss, {:.1} t, self-weight {:.1} kN",
        spec.span(),
        mass / 1000.0,
        mass * GRAVITY.length() / 1000.0
    );
    println!(
        "  Timoshenko beam theory (bending + shear): {:.2} mm",
        analytic * 1000.0
    );
    println!(
        "  Exact linear FEM of the same truss (direct stiffness): {:.2} mm  ({:.3}x beam theory)",
        exact * 1000.0,
        exact / analytic
    );
    println!("  substeps | XPBD sag (mm) | ratio to exact FEM | cpu time for 15 s");
    for (substeps, name) in [
        (8, "xpbd_sag_ratio_8sub"),
        (20, "xpbd_sag_ratio_20sub"),
        (50, "xpbd_sag_ratio_50sub"),
        (100, "xpbd_sag_ratio_100sub"),
        (200, "xpbd_sag_ratio_200sub"),
    ] {
        let start = Instant::now();
        let sag = measure_static_sag(substeps);
        let elapsed = start.elapsed();
        println!(
            "  {substeps:>8} | {:>13.2} | {:>18.3} | {:.0} ms",
            sag * 1000.0,
            sag / exact,
            elapsed.as_secs_f64() * 1000.0
        );
        // Accuracy metrics are tracked for regressions (closer to 1 is better; XPBD only
        // ever comes out too soft, so lower is better).
        report.metric(name, sag / exact, Better::Lower);
    }
    println!("  (accuracy findings and decision: docs/adr/0002-physics-solver.md)");
    println!();
}

fn spike_b_meshing(report: &mut Report, threads: usize) {
    let planet = PlanetDensity::reference(1);
    let origins = surface_chunk_origins(&planet, 512, 1.0);
    println!(
        "Spike (b) voxel meshing: {CHUNK_CELLS}^3-cell chunks at 1 m on a {:.0} km planet",
        planet.radius / 1000.0
    );

    // One thread: split generation (noise sampling) from meshing.
    let mut sample_times = TimingStats::new(128);
    let mut mesh_times = TimingStats::new(128);
    let mut triangles = 0usize;
    for &origin in &origins[..128] {
        let start = Instant::now();
        let chunk = planet.sample_chunk(origin, 1.0);
        sample_times.push(start.elapsed());
        let start = Instant::now();
        let mesh = chunk.mesh();
        mesh_times.push(start.elapsed());
        triangles += mesh.triangle_count();
    }
    let generate_ms = sample_times.mean_ms().unwrap_or(f64::NAN);
    let mesh_ms = mesh_times.mean_ms().unwrap_or(f64::NAN);
    println!(
        "  one thread: generate {generate_ms:.2} ms + mesh {mesh_ms:.2} ms per chunk (mean), {} triangles/chunk",
        triangles / 128
    );

    // All threads.
    let start = Instant::now();
    let total: usize = origins
        .par_iter()
        .map(|&origin| planet.sample_chunk(origin, 1.0).mesh().triangle_count())
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let rate = origins.len() as f64 / start.elapsed().as_secs_f64();
    #[allow(clippy::cast_precision_loss)]
    let million_triangles = total as f64 / 1e6;
    println!("  {threads} threads: {rate:.0} chunks/s ({million_triangles:.1} M triangles)");

    let projected = 1000.0 / (generate_ms + mesh_ms) * 6.0 * MIN_SPEC_CORE_FACTOR;
    report.metric("terrain_generate_ms_per_chunk", generate_ms, Better::Lower);
    report.metric("terrain_mesh_ms_per_chunk", mesh_ms, Better::Lower);
    report.metric("terrain_chunks_per_s_all_threads", rate, Better::Higher);
    report.budget(
        projected >= MESHING_TARGET_CHUNKS_PER_S,
        format!(
            "meshing projected to 6 min-spec cores: {projected:.0} chunks/s >= {MESHING_TARGET_CHUNKS_PER_S}"
        ),
    );
    println!();
}
