//! One island's backward-Euler step, by Newton's method.
//!
//! For each mass i: mᵢ·(H(Tᵢ) − Hᵢ⁰) = Δt·(sources − Σ flows out of i) + heat received.
//!
//! - Conduction and convection coefficients are lagged: taken at the step's start (as thermal
//!   network codes do: SINDA, TRNSYS), and re-evaluated only once the temperatures they
//!   depend on have moved enough to change them by a fraction of a percent
//!   ([`crate::kernel`]). The temperatures they multiply stay implicit, so the step stays
//!   unconditionally stable, and a network of them alone is linear: one solve a step.
//! - Radiation is exact: to a fixed node with its derivative 4σF·T³; between two masses with
//!   its secant σF·(T₁² + T₂²)(T₁ + T₂), which keeps the matrix symmetric for the Cholesky
//!   solve.
//! - Melting is exact, through the enthalpy: an iterate crossing a melting range's end lands
//!   on it ([`land`]).
//!
//! Newton starts from the last step's trend carried on, and stops once the residual bounds the
//! error below [`TOLERANCE`]. A factorisation is kept while it is exact, or while it is close
//! ([`REUSE`]), converging fast, and dearer to remake than a solve. Each node's enthalpy then
//! changes by exactly the heat booked into it: the ledger closes to rounding whatever the
//! tolerance.

mod iterate;

use crate::enthalpy::Enthalpy;
use crate::kernel::{Cold, Ends, FIXED, Hot, PARALLEL_FROM, TO_FIXED, each2, radiation, refresh};
use crate::network::{Link, NodeId, NodeKind};
use creeer_math::sparse::{Assembly, Cholesky, Csc};
use creeer_network::{Sleep, Sum};
use std::sync::Arc;

pub use iterate::{REUSE, TOLERANCE};

/// Iterations at most.
const MAX_ITERATIONS: usize = 40;
/// No node changes by more than this in one iteration (K).
const MOST_CHANGE: f64 = 50.0;
/// An island sleeps once no temperature changes by more than this a step (K).
const STILL: f64 = 1e-6;

/// The network as an island's solve reads it.
pub(crate) struct Context<'a> {
    pub temperature: &'a [f64],
    pub enthalpy: &'a [f64],
    pub source: &'a [f64],
    pub received: &'a [f64],
    pub links: &'a [Link],
    pub gravity: f64,
}

/// What an island's step did with energy, and how hard it was.
pub(crate) struct Outcome {
    pub supplied: f64,
    pub imported: f64,
    pub dissipated: f64,
    pub iterations: usize,
    pub factorisations: usize,
    pub converged: bool,
}

/// A node's state in an iteration.
#[derive(Clone, Copy, Debug, Default)]
struct NodeState {
    /// Its temperature, and its specific enthalpy at the step's start.
    t: f64,
    h0: f64,
    /// Heat in this step from sources and other domains, and with the links' flows (J).
    heat: f64,
    gain: f64,
    /// m·(H(T) − H⁰) − gain, and m·dH/dT.
    residual: f64,
    capacity: f64,
    /// Heat it gave fixed nodes (J), its capacity in the factorisation, and its
    /// temperature change last step (Newton's predictor).
    lost: f64,
    factored: f64,
    trend: f64,
}

/// One island's solver.
#[derive(Debug)]
pub(crate) struct IslandSolver {
    pub nodes: Vec<NodeId>,
    links: Vec<usize>,
    ends: Vec<Ends>,
    /// σ·F of each link that radiates, zero for the others.
    radiation: Vec<f64>,
    mass: Vec<f64>,
    enthalpy: Vec<Arc<Enthalpy>>,
    /// Each node's links in link order, [`FIXED`] set where the node is the link's first,
    /// [`TO_FIXED`] where its second is fixed.
    incident_ptr: Vec<usize>,
    incident: Vec<u32>,
    assembly: Assembly,
    matrix: Csc<f64>,
    cholesky: Option<Cholesky>,
    /// The step length the factorisation was made for (each element keeps its own values).
    factored_dt: f64,
    contributions: Vec<f64>,
    sleep: Sleep,
    /// The fixed temperatures and sources it last saw (a change wakes it).
    inputs: Vec<f64>,
    seen: Vec<f64>,
    node_state: Vec<NodeState>,
    hot: Vec<Hot>,
    cold: Vec<Cold>,
    delta: Vec<f64>,
    /// The new temperatures and specific enthalpies, after a step.
    pub t: Vec<f64>,
    pub h: Vec<f64>,
}

/// Newton's iterate for a node moving from `t` to `next`, stopped at the first end of its
/// melting range `(lo, hi)` it would cross. H(T) has corners there; a step across one uses a
/// slope that is wrong past it, and from both sides Newton can jump back and forth across the
/// range forever. Landed on the corner, the next step takes the corner's larger slope, which is
/// never too long, and then the slope beyond.
fn land(t: f64, next: f64, (lo, hi): (f64, f64)) -> f64 {
    if next > t {
        [lo, hi]
            .into_iter()
            .find(|&k| t < k && k < next)
            .unwrap_or(next)
    } else {
        [hi, lo]
            .into_iter()
            .find(|&k| next < k && k < t)
            .unwrap_or(next)
    }
}

impl IslandSolver {
    /// The solver of the island of masses `nodes` (`local` gives a node's place there) and
    /// links `links` (indices into `all`, each starting at one of them).
    ///
    /// # Panics
    /// If a node's or link's id does not fit in 30 bits.
    pub fn new(
        nodes: Vec<NodeId>,
        links: Vec<usize>,
        kinds: &[NodeKind],
        all: &[Link],
        local: impl Fn(NodeId) -> usize,
    ) -> Self {
        let n = nodes.len();
        let (mut mass, mut enthalpy) = (Vec::with_capacity(n), Vec::with_capacity(n));
        for &node in &nodes {
            let NodeKind::Mass {
                mass: m,
                enthalpy: h,
            } = &kinds[node]
            else {
                unreachable!("an island is of masses")
            };
            mass.push(*m);
            enthalpy.push(h.clone());
        }
        let id = |k: usize| {
            u32::try_from(k)
                .ok()
                .filter(|&k| k & (FIXED | TO_FIXED) == 0)
                .expect("ids fit in 30 bits")
        };
        let mut positions: Vec<(usize, usize)> = (0..n).map(|k| (k, k)).collect();
        let mut ends = Vec::with_capacity(links.len());
        let mut per_node: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (j, &k) in links.iter().enumerate() {
            let (a, b) = all[k].nodes();
            let la = local(a);
            let to_fixed = if matches!(kinds[b], NodeKind::Fixed) {
                TO_FIXED
            } else {
                0
            };
            per_node[la].push(id(j) | FIXED | to_fixed);
            if matches!(kinds[b], NodeKind::Mass { .. }) {
                let lb = local(b);
                per_node[lb].push(id(j));
                positions.extend([(la, la), (lb, lb), (la, lb), (lb, la)]);
                ends.push(Ends(id(la), id(lb)));
            } else {
                positions.push((la, la));
                ends.push(Ends(id(la), id(b) | FIXED));
            }
        }
        let mut incident_ptr = vec![0];
        let mut incident = Vec::new();
        for list in per_node {
            incident.extend(list);
            incident_ptr.push(incident.len());
        }
        let assembly = Assembly::new(n, &positions);
        Self {
            radiation: links.iter().map(|&k| radiation(&all[k])).collect(),
            hot: vec![Hot::default(); links.len()],
            cold: vec![Cold::default(); links.len()],
            node_state: vec![NodeState::default(); n],
            matrix: assembly.zeros(),
            assembly,
            nodes,
            links,
            ends,
            mass,
            enthalpy,
            incident_ptr,
            incident,
            cholesky: None,
            factored_dt: f64::NAN,
            contributions: Vec::new(),
            sleep: Sleep::default(),
            inputs: Vec::new(),
            seen: Vec::new(),
            delta: vec![0.0; n],
            t: Vec::new(),
            h: Vec::new(),
        }
    }

    /// Whether it sleeps (and nothing acting on it changed since its last step: fixed
    /// temperatures, sources, heat received). A change wakes it.
    fn sleeping(&mut self, c: &Context<'_>) -> bool {
        self.seen.clear();
        self.seen.extend(
            self.ends
                .iter()
                .filter_map(|e| e.b().err().map(|b| c.temperature[b])),
        );
        self.seen.extend(self.nodes.iter().map(|&n| c.source[n]));
        if self.seen != self.inputs || self.nodes.iter().any(|&n| c.received[n] != 0.0) {
            self.sleep.wake();
            std::mem::swap(&mut self.inputs, &mut self.seen);
            return false;
        }
        self.sleep.asleep()
    }

    /// Whether it is being solved.
    pub fn awake(&self) -> bool {
        !self.sleep.asleep()
    }

    /// One step of `dt`, or none if it sleeps.
    pub fn step(&mut self, c: &Context<'_>, dt: f64) -> Option<Outcome> {
        if self.sleeping(c) {
            return None;
        }
        let parallel = self.nodes.len() >= PARALLEL_FROM;
        for (s, &node) in self.node_state.iter_mut().zip(&self.nodes) {
            (s.t, s.h0) = (c.temperature[node], c.enthalpy[node]);
            s.heat = c.source[node] * dt + c.received[node];
        }
        let (nodes, ends, links) = (&self.node_state, &self.ends, &self.links);
        each2(&mut self.hot, &mut self.cold, parallel, |j, hot, cold| {
            let e = ends[j];
            let tb = e.b().map_or_else(|b| c.temperature[b], |b| nodes[b].t);
            refresh(hot, cold, &c.links[links[j]], nodes[e.a()].t, tb, c.gravity);
        });
        // Newton starts from the last step's trend carried on: its error is the trend's change,
        // so even a stale factorisation's first correction is usually the last one needed.
        for s in &mut self.node_state {
            s.t += s.trend;
        }
        let (mut iterations, mut factorisations) = (0, 0);
        let (mut last_error, mut stale) = (f64::INFINITY, false);
        let converged = loop {
            let (error, drift) = self.evaluate(c, dt, parallel);
            if error < TOLERANCE {
                break true;
            }
            if iterations == MAX_ITERATIONS {
                break false;
            }
            iterations += 1;
            let (made, was_stale) =
                self.factorise(dt, drift, stale && error > iterate::SLOW * last_error);
            factorisations += usize::from(made);
            (last_error, stale) = (error, was_stale);
            for (d, s) in self.delta.iter_mut().zip(&self.node_state) {
                *d = -s.residual;
            }
            self.cholesky
                .as_mut()
                .expect("factorised")
                .solve(&mut self.delta);
            let largest = self.delta.iter().fold(0.0f64, |m, d| m.max(d.abs()));
            let scale = if largest > MOST_CHANGE {
                MOST_CHANGE / largest
            } else {
                1.0
            };
            for ((s, d), h) in self
                .node_state
                .iter_mut()
                .zip(&self.delta)
                .zip(&self.enthalpy)
            {
                let next = s.t + d * scale;
                s.t = h.melting().map_or(next, |range| land(s.t, next, range));
            }
        };
        // Each node's enthalpy changes by exactly the heat booked into it, and its temperature
        // moves the last ≤ TOLERANCE onto that enthalpy (its residual over its capacity).
        let (mut change, mut supplied, mut imported) = (0.0f64, 0.0, 0.0);
        self.t.clear();
        self.h.clear();
        for (i, (s, &node)) in self.node_state.iter_mut().zip(&self.nodes).enumerate() {
            let t = s.t - s.residual / s.capacity;
            s.trend = t - c.temperature[node];
            change = change.max(s.trend.abs());
            self.t.push(t);
            self.h.push(s.h0 + s.gain / self.mass[i]);
            supplied += c.source[node] * dt;
            imported += c.received[node];
        }
        let mut dissipated = Sum::default();
        for s in &self.node_state {
            dissipated.add(s.lost);
        }
        self.sleep.after_step(change, STILL);
        let dissipated = dissipated.value();
        Some(Outcome {
            supplied,
            imported,
            dissipated,
            iterations,
            factorisations,
            converged,
        })
    }
}
