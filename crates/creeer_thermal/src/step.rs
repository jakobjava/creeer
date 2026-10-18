//! A step of the thermal network: backward Euler (unconditionally stable for the stiffest
//! network, and conservative: what a step's flows carry is exactly what the nodes' enthalpies
//! change by), island by island, in parallel ([`crate::island`] has the method).

use crate::island::{Context, IslandSolver};
use crate::network::{Link, Network, NodeId, NodeKind};
use creeer_network::Islands;
use rayon::prelude::*;

/// What a step did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepReport {
    /// Islands solved (not asleep).
    pub solved: usize,
    /// Newton iterations at most in one island.
    pub iterations: usize,
    /// Factorisations made.
    pub factorisations: usize,
    /// Islands whose Newton iteration did not converge in its iterations (their energy is
    /// still booked exactly; their temperatures are a step's error off).
    pub unconverged: usize,
}

impl Network {
    /// Builds the islands and their solvers.
    fn build(&self) -> (Islands, Vec<IslandSolver>) {
        let is_mass = |n: NodeId| matches!(self.kinds[n], NodeKind::Mass { .. });
        let edges = self
            .links
            .iter()
            .map(Link::nodes)
            .filter(|&(a, b)| is_mass(a) && is_mass(b));
        let islands = Islands::new(self.kinds.len(), edges);
        let mut links_of: Vec<Vec<usize>> = vec![Vec::new(); islands.count()];
        for (k, link) in self.links.iter().enumerate() {
            links_of[islands.island_of(link.nodes().0)].push(k);
        }
        // In parallel, each island by itself, collected in order (a 20,000-node hall's 1,500
        // islands took 22 ms on one thread, its first step's).
        let solvers = std::mem::take(&mut links_of)
            .into_par_iter()
            .enumerate()
            .filter(|(i, _)| is_mass(islands.members(*i)[0]))
            .map(|(i, links)| {
                let nodes = islands.members(i).to_vec();
                IslandSolver::new(nodes, links, &self.kinds, &self.links, |n| islands.local(n))
            })
            .collect();
        (islands, solvers)
    }

    /// Advances the network by `dt` seconds.
    ///
    /// # Panics
    /// If an island's matrix is not positive definite (a mass node without heat capacity).
    pub fn step(&mut self, dt: f64) -> StepReport {
        let (islands, mut solvers) = match self.islands.take() {
            Some(built) => built,
            None => self.build(),
        };
        let received: Vec<f64> = self
            .received
            .iter_mut()
            .map(|r| std::mem::take(r).value())
            .collect();
        let context = Context {
            temperature: &self.temperature,
            enthalpy: &self.enthalpy,
            source: &self.source,
            received: &received,
            links: &self.links,
            gravity: self.gravity,
        };
        let outcomes: Vec<_> = solvers
            .par_iter_mut()
            .map(|s| s.step(&context, dt))
            .collect();
        let mut report = StepReport::default();
        for (solver, outcome) in solvers.iter().zip(outcomes) {
            let Some(o) = outcome else { continue };
            for ((&node, &t), &h) in solver.nodes.iter().zip(&solver.t).zip(&solver.h) {
                self.temperature[node] = t;
                self.enthalpy[node] = h;
            }
            self.ledger.supply(o.supplied);
            self.ledger.import(o.imported);
            self.ledger.dissipate(o.dissipated);
            report.solved += 1;
            report.iterations = report.iterations.max(o.iterations);
            report.factorisations += o.factorisations;
            report.unconverged += usize::from(!o.converged);
        }
        self.islands = Some((islands, solvers));
        report
    }
}
