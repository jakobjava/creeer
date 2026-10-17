//! A circuit's step: its islands (the parts only ground joins: each its own system) solved
//! in parallel, then every component finished in order and its energy booked.

use crate::circuit::Circuit;
use crate::component::{Component, GROUND, Kind};
use crate::finish::{Flow, finish};
use crate::mna::{GMIN, Place};
use crate::solve::IslandSolver;
use creeer_network::{Islands, Sum};
use rayon::prelude::*;

/// What a step did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepReport {
    /// Islands solved (not asleep).
    pub solved: usize,
    /// Newton iterations at most in one island, and factorisations made.
    pub iterations: usize,
    pub factorisations: usize,
    /// Islands whose Newton iteration did not converge.
    pub unconverged: usize,
    /// Components that switched (fuses, breakers, battery protection).
    pub switched: usize,
}

impl Circuit {
    /// The islands: connected through any node but ground.
    fn build(&self) -> Vec<IslandSolver> {
        let edges = self
            .components
            .iter()
            .filter(|c| c.a != GROUND && c.b != GROUND)
            .map(|c| (c.a, c.b));
        let islands = Islands::new(self.nodes, edges);
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); islands.count()];
        for (k, c) in self.components.iter().enumerate() {
            let node = if c.a == GROUND { c.b } else { c.a };
            members[islands.island_of(node)].push(k);
        }
        let mut local = vec![None; self.nodes];
        let used: Vec<usize> = (0..islands.count())
            .filter(|&i| !members[i].is_empty())
            .collect();
        used.into_iter()
            .map(|i| {
                let nodes: Vec<usize> = islands
                    .members(i)
                    .iter()
                    .copied()
                    .filter(|&n| n != GROUND)
                    .collect();
                for (k, &n) in nodes.iter().enumerate() {
                    local[n] = Some(k);
                }
                let mut solver = IslandSolver::new(
                    &self.components,
                    std::mem::take(&mut members[i]),
                    nodes,
                    &local,
                );
                // Start from the circuit's voltages and the components' states.
                for (k, &n) in solver.nodes.iter().enumerate() {
                    solver.x[k] = self.voltage[n];
                }
                // An electrolyser's cells at rest (its internal node at cells·E above its minus):
                // from nought they would start a volt and more from it, at 10¹³ A, and Newton
                // walk back one b (26 mV) an iteration.
                for (&k, place) in solver.components.iter().zip(&solver.places) {
                    if let Kind::Electrolyzer(cell) = &self.components[k].kind {
                        let below = place.b.map_or(0.0, |i| solver.x[i]);
                        solver.x[place.extra] =
                            below + cell.cells * self.components[k].state.extra[0];
                    }
                }
                solver
            })
            .collect()
    }

    /// Advances the circuit by `dt` seconds.
    pub fn step(&mut self, dt: f64) -> StepReport {
        let mut islands = self.islands.take().unwrap_or_else(|| self.build());
        let components = &self.components;
        let outcomes: Vec<_> = islands
            .par_iter_mut()
            .map(|s| (!s.asleep).then(|| s.solve(components, dt)))
            .collect();

        let mut report = StepReport::default();
        let mut totals = Totals::default();
        for (island, outcome) in islands.iter_mut().zip(outcomes) {
            let Some(o) = outcome else {
                // Asleep: nothing moves, the same energy flows as the last step.
                totals.dissipated.add(island.leak);
                for (&k, flow) in island.components.iter().zip(&island.flows) {
                    totals.book(flow);
                    self.components[k].state.heat.add(flow.heat);
                    self.components[k]
                        .state
                        .work
                        .add(flow.exported - flow.imported);
                }
                continue;
            };
            report.solved += 1;
            report.iterations = report.iterations.max(o.iterations);
            report.factorisations += o.factorisations;
            report.unconverged += usize::from(!o.converged);
            // GMIN's leakage (1 TΩ from every node to ground) is a conductance too.
            // Booked as the components are: averaged over the step during BDF2.
            let power: f64 = (0..island.nodes.len())
                .map(|k| GMIN * island.x[k] * island.x[k])
                .sum();
            #[allow(clippy::float_cmp)] // exactly BDF2's coefficient
            let leak = if island.rule.a0 == 1.5 {
                0.5 * (island.leak_power + power) * dt
            } else {
                power * dt
            };
            (island.leak, island.leak_power) = (leak, power);
            totals.dissipated.add(leak);
            for (k, &n) in island.nodes.iter().enumerate() {
                self.voltage[n] = island.x[k];
            }
            let (x, rule) = (&island.x, island.rule);
            let one = |c: &mut Component, place: Place, slot: &mut Flow| {
                let (flow, s) = finish(c, place, x, rule);
                c.state.heat.add(flow.heat);
                c.state.work.add(flow.exported - flow.imported);
                *slot = flow;
                (usize::from(s), usize::from(crate::finish::drifting(c)))
            };
            let add = |(a, b): (usize, usize), (c, d): (usize, usize)| (a + c, b + d);
            let (flips, drifting) = if island.components.len() >= PARALLEL_FINISH {
                // Each component's finish touches its own state alone: in parallel.
                let mut own = members(&mut self.components, &island.components);
                own.par_iter_mut()
                    .zip(island.places.par_iter())
                    .zip(island.flows.par_iter_mut())
                    .with_min_len(256)
                    .map(|((c, &place), slot)| one(c, place, slot))
                    .reduce(|| (0, 0), add)
            } else {
                let mut counts = (0, 0);
                for ((&k, &place), slot) in island
                    .components
                    .iter()
                    .zip(&island.places)
                    .zip(&mut island.flows)
                {
                    counts = add(counts, one(&mut self.components[k], place, slot));
                }
                counts
            };

            // The energy summed in fixed chunks, then the chunks in order: the same bits
            // however many threads did them.
            let chunks: Vec<Totals> = island
                .flows
                .par_chunks(CHUNK)
                .map(|chunk| {
                    let mut t = Totals::default();
                    for f in chunk {
                        t.book(f);
                    }
                    t
                })
                .collect();
            for chunk in &chunks {
                totals.merge(chunk);
            }
            report.switched += flips;
            let switched = flips > 0;

            if switched {
                island.restart();
            } else {
                island.settle(if drifting > 0 {
                    f64::INFINITY
                } else {
                    island.moved
                });
            }
        }

        self.ledger.supply(totals.supplied.value());
        self.ledger.dissipate(totals.dissipated.value());
        self.ledger.export(totals.exported.value());
        self.ledger.import(totals.imported.value());
        self.islands = Some(islands);
        report
    }
}

/// A step's energy, summed over its components.
#[derive(Default)]
struct Totals {
    supplied: Sum,
    dissipated: Sum,
    exported: Sum,
    imported: Sum,
}

impl Totals {
    fn book(&mut self, flow: &Flow) {
        self.supplied.add(flow.supplied);
        self.dissipated.add(flow.dissipated);
        self.exported.add(flow.exported);
        self.imported.add(flow.imported);
    }
}

/// Islands of at least this many components finish them in parallel.
const PARALLEL_FINISH: usize = 2048;

/// The components `ids` (ascending) of `all`, each borrowed alone.
fn members<'a, T>(all: &'a mut [T], ids: &[usize]) -> Vec<&'a mut T> {
    let mut out = Vec::with_capacity(ids.len());
    let mut wanted = ids.iter().peekable();
    for (k, c) in all.iter_mut().enumerate() {
        if wanted.peek() == Some(&&k) {
            out.push(c);
            wanted.next();
        }
    }
    out
}

/// Flows summed per chunk of this many components.
const CHUNK: usize = 1024;

impl Totals {
    fn merge(&mut self, other: &Self) {
        self.supplied.add(other.supplied.value());
        self.dissipated.add(other.dissipated.value());
        self.exported.add(other.exported.value());
        self.imported.add(other.imported.value());
    }
}
