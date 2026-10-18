//! A member's plastic split started where its last split left the end-moment pairs (warm,
//! as every evaluation within a step does) finds the plastic part a split from nought finds,
//! to rounding (F7.6, ADR-0021): the same minimum of the same convex potential.

use super::element::MEASURES;
use super::flow::Flow;
use super::plastic::Guess;
use super::section::STEEL_S235;
use super::shapes::{Base, space_frame};
use super::step_flows::StepFlows;
use super::*;

/// A deterministic sequence in [0, 1).
fn uniform(state: &mut u64) -> f64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    (*state >> 11) as f64 / (1u64 << 53) as f64
}

#[test]
fn warm_and_cold_splits_find_the_same_plastic_part() {
    let tube = Section::round_tube(0.08, 0.004);
    for material in [STEEL_S235, STEEL_S235.perfectly_plastic()] {
        let s = space_frame([2, 1, 1], 2.0, tube, material, Base::Free);
        let e = &s.elements[0];
        let (h, beta) = (1.0 / 60.0 / 34.0, 1.0 / 60.0 / 34.0 / 2.0);
        let gamma = e.material.damping / beta;
        let zero = [0.0; MEASURES];
        let limits = e.limits(&zero, gamma);
        // The measure at which each first flows alone.
        let yields: [f64; MEASURES] = std::array::from_fn(|m| limits[m] / e.stiffness.entry(m, m));
        let split = |measures: &[f64; MEASURES], guess: &mut Guess| {
            let (mut flows, mut built) = ([[Flow::NEVER; MEASURES]; 2], false);
            let mut step = StepFlows {
                limits: &limits,
                s_hat: &zero,
                step: (h, beta),
                flows: &mut flows,
                built: &mut built,
            };
            e.plastic_split_from(measures, &mut step, guess)
        };
        let (mut state, mut worst, mut flowed) = (7u64, 0.0f64, 0);
        for _ in 0..4000 {
            // Up to four times the yield measure, either sign, every measure at once.
            let measures: [f64; MEASURES] =
                std::array::from_fn(|m| yields[m] * 8.0 * (uniform(&mut state) - 0.5));
            let cold = split(&measures, &mut [0.0; 2 * MEASURES]);
            // Warm: from the split of a state nearby (a trial of the step before).
            let mut guess = [0.0; 2 * MEASURES];
            let near = measures.map(|v| v * (1.0 + 0.01 * (uniform(&mut state) - 0.5)));
            let _ = split(&near, &mut guess);
            let warm = split(&measures, &mut guess);
            for m in 0..MEASURES {
                let off = (warm[m] - cold[m]).abs() / yields[m].abs();
                worst = worst.max(off);
            }
            flowed += usize::from(cold.iter().any(|p| *p != 0.0));
        }
        assert!(flowed > 3000, "{flowed} of 4000 splits flowed");
        // Measured: 4.2e-15 rate-lifted (the flow's Newton stops within 1e-15 of w), 1.6e-14
        // perfectly plastic: a split from nought stops at its 24 sweeps, each contracting by
        // (k12/k11)^2 = 1/4, before the fixed point, 3 yield measures * 4^-24 = 1.1e-14 away;
        // from nearby it reaches it. Both a few dozen roundings of the measure.
        assert!(
            worst < 1e-13,
            "warm and cold splits differ by {worst:e} of a yield measure"
        );
    }
}
