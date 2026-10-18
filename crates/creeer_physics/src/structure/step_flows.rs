//! A member's plastic flows in one step, built only when needed.
//!
//! A step's prediction gives each member its limits (`Work::limits`: its capacities over
//! 1 + γ); a member whose measures stay within them is elastic, and that is all of its flows
//! an evaluation reads. The flows themselves (`Element::capacities` at ŝ, and the same with
//! their work divided by 1 + γ) are built the first time a measure passes its limit in the
//! step, from what the step began with: the same flows, bit for bit, as building them all
//! at its start, which cost a 5,000-member bridge 1.2 kB of memory traffic a member a step.

use super::Structure;
use super::element::{Element, MEASURES};
use super::flow::Flow;
use super::solver::Work;

/// One member's flows in one step.
pub(crate) struct StepFlows<'a> {
    pub limits: &'a [f64; MEASURES],
    pub s_hat: &'a [f64; MEASURES],
    /// The step and β the limits were found with (`Work::flow_step`).
    pub step: (f64, f64),
    /// The flows and the same with their work divided by 1 + γ, if `built`.
    pub flows: &'a mut [[Flow; MEASURES]; 2],
    pub built: &'a mut bool,
}

impl StepFlows<'_> {
    /// Member `index`'s in `work`, its flows built into `flows` and `built`.
    pub(super) fn of<'a>(
        work: &'a Work,
        index: usize,
        (flows, built): (&'a mut [[Flow; MEASURES]; 2], &'a mut bool),
    ) -> StepFlows<'a> {
        StepFlows {
            limits: &work.limits[index],
            s_hat: &work.s_hat[index],
            step: work.flow_step,
            flows,
            built,
        }
    }

    /// The flows, built now if not yet (from the member as the step began).
    pub(crate) fn get(&mut self, e: &Element) -> &[[Flow; MEASURES]; 2] {
        if !*self.built {
            let (h, beta) = self.step;
            let caps = e.capacities(self.s_hat, h);
            let gamma = e.material.damping / beta;
            *self.flows = [caps, caps.map(|f| f.scaled(1.0 + gamma))];
            *self.built = true;
        }
        self.flows
    }

    /// The plastic work of increments `p` (only measures that flowed: 0·∞ would be NaN for
    /// members that never yield).
    pub(crate) fn work(&mut self, e: &Element, p: &[f64; MEASURES]) -> f64 {
        if p.iter().all(|p| *p == 0.0) {
            return std::iter::empty::<f64>().sum();
        }
        p.iter()
            .zip(&self.get(e)[0])
            .filter(|(p, _)| **p != 0.0)
            .map(|(p, f)| f.work(p.abs()))
            .sum()
    }
}

impl Work {
    /// Builds every member's flows not built yet: before the measures they come from are
    /// overwritten outside a step (`ImplicitSolver::initial_accelerations`), whose
    /// evaluation goes on with the last step's flows.
    pub(super) fn build_flows(&mut self, s: &Structure) {
        let (mut flows, mut built) = (
            std::mem::take(&mut self.flows),
            std::mem::take(&mut self.built),
        );
        for (index, e) in s.elements.iter().enumerate() {
            StepFlows::of(self, index, (&mut flows[index], &mut built[index])).get(e);
        }
        (self.flows, self.built) = (flows, built);
    }
}
