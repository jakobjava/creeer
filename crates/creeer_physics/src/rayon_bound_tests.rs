//! (P1 exploration, ADR-0024.) What a parallel region costs in a Newton iteration's rhythm:
//! three regions of `work` µs of evaluation each, split over the pool, with `gap` µs of serial
//! work between, against the same done serially.

use rayon::prelude::*;
use std::time::Instant;

/// Busy work of about `ns` nanoseconds (an evaluation's share).
fn spin(ns: u64, seed: f64) -> f64 {
    let start = Instant::now();
    let mut x = seed;
    while start.elapsed().as_nanos() < u128::from(ns) {
        for _ in 0..50 {
            x = x.mul_add(1.000_000_1, 1e-9);
        }
    }
    x
}

#[test]
#[ignore = "a measurement, run by hand"]
fn a_parallel_region_in_a_newton_iterations_rhythm() {
    let pieces = 112; // a ragdoll group's contacts
    for (work_us, gap_us) in [(20u64, 10u64), (10, 10), (5, 5)] {
        let per = work_us * 1000 / pieces as u64;
        let iterations = 2000;
        let serial = {
            let t = Instant::now();
            let mut s = 0.0;
            for _ in 0..iterations {
                for _ in 0..3 {
                    s += (0..pieces).map(|i| spin(per, i as f64)).sum::<f64>();
                    s += spin(gap_us * 1000, s);
                }
            }
            std::hint::black_box(s);
            t.elapsed().as_secs_f64() * 1e6 / f64::from(iterations)
        };
        let parallel = {
            let t = Instant::now();
            let mut s = 0.0;
            for _ in 0..iterations {
                for _ in 0..3 {
                    s += (0..pieces)
                        .into_par_iter()
                        .map(|i| spin(per, i as f64))
                        .sum::<f64>();
                    s += spin(gap_us * 1000, s);
                }
            }
            std::hint::black_box(s);
            t.elapsed().as_secs_f64() * 1e6 / f64::from(iterations)
        };
        let ideal = 3.0 * (gap_us as f64 + work_us as f64 / rayon::current_num_threads() as f64);
        eprintln!(
            "3 regions of {work_us} µs, {gap_us} µs between: serial {serial:.1} µs, parallel {parallel:.1} µs an iteration (ideal {ideal:.1}, {} threads)",
            rayon::current_num_threads()
        );
    }
}
