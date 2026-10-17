//! What pipes carry (F6.6): a fluid's content, any quantities that go with its mass (moles of
//! each species per kilogram, enthalpy per kilogram), carried by the flows a network's step
//! found. Mixed by mass wherever streams meet, so every quantity is conserved exactly.
//!
//! - **Pipes** that hold fluid (a liquid's) are plug flow: their content is a train of
//!   parcels, fed at the upstream end and drawn off at the downstream end (EPANET's
//!   Lagrangian method), no numerical smearing however long the line.
//! - **Junctions** (no storage) mix their inflows at once; **stores** (tanks, a gas line's
//!   sections) are well mixed; **sources** (reservoirs, supplies) give a set content and take
//!   whatever reaches them.
//! - Links that hold nothing (pumps, valves, a gas pipe whose volume its ends store) pass their
//!   upstream node's content straight on, so nodes are settled upstream first.
//!
//! A step is split so no pipe is flushed of more than half its fluid in a part: then each
//! part's outflows are fluid that was already there, and the order exact.

use creeer_network::{Ledger, Sum};
use std::collections::VecDeque;

mod step;
#[cfg(test)]
mod tests;

/// Parcels whose contents differ by less than this (relative) are one.
const SAME: f64 = 1e-12;

/// Parts a step is split into at most.
const PARTS: usize = 4096;

/// What a node of a carried network is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holding {
    /// Its content set (a reservoir, a supply).
    Source,
    /// Holds nothing: its outflow is its inflows mixed (a junction).
    Mix,
    /// Holds fluid, well mixed (a tank, a gas store).
    Store,
}

#[derive(Clone, Debug)]
struct Point {
    holding: Holding,
    mass: f64,
    content: Vec<f64>,
}

#[derive(Clone, Debug)]
struct Line {
    a: usize,
    b: usize,
    /// The fluid it holds (kg); nought passes content straight on.
    holdup: f64,
    /// Its parcels from `a` to `b`, each its mass then its content.
    parcels: VecDeque<f64>,
}

/// A network's content.
#[derive(Clone, Debug)]
pub struct Tracer {
    k: usize,
    points: Vec<Point>,
    lines: Vec<Line>,
    /// The mass's ledger and each quantity's.
    ledgers: Vec<Ledger>,
    scratch: step::Scratch,
    /// The flows and draws a network gives it, kept between steps (`carry.rs`).
    pub(crate) given: (Vec<f64>, Vec<f64>),
}

impl Tracer {
    /// A network carrying `quantities` quantities per kilogram.
    pub fn new(quantities: usize) -> Self {
        Self {
            k: quantities,
            points: Vec::new(),
            lines: Vec::new(),
            ledgers: vec![Ledger::new(0.0); quantities + 1],
            scratch: step::Scratch::default(),
            given: (Vec::new(), Vec::new()),
        }
    }

    /// The number of quantities carried.
    pub fn quantities(&self) -> usize {
        self.k
    }

    /// Adds a node holding `mass` (kg; a store's) of `content`.
    pub fn node(&mut self, holding: Holding, mass: f64, content: &[f64]) -> usize {
        assert_eq!(content.len(), self.k, "a content of each quantity");
        let mass = if holding == Holding::Store { mass } else { 0.0 };
        self.join(mass, content);
        self.points.push(Point {
            holding,
            mass,
            content: content.to_vec(),
        });
        self.points.len() - 1
    }

    /// Adds a link from `a` to `b` holding `holdup` kg, all of `content`.
    pub fn link(&mut self, a: usize, b: usize, holdup: f64, content: &[f64]) -> usize {
        assert_eq!(content.len(), self.k, "a content of each quantity");
        let mut parcels = VecDeque::new();
        if holdup > 0.0 {
            parcels.push_back(holdup);
            parcels.extend(content);
            self.join(holdup, content);
        }
        self.lines.push(Line {
            a,
            b,
            holdup,
            parcels,
        });
        self.lines.len() - 1
    }

    fn join(&mut self, mass: f64, content: &[f64]) {
        self.ledgers[0].joined(mass);
        for (l, c) in self.ledgers[1..].iter_mut().zip(content) {
            l.joined(mass * c);
        }
    }

    /// Sets a source's content (what a reservoir, a supply, a reactor's outlet gives).
    pub fn set(&mut self, node: usize, content: &[f64]) {
        assert_eq!(content.len(), self.k, "a content of each quantity");
        self.points[node].content.copy_from_slice(content);
    }

    /// Adds `amount` of quantity `q` (0 the first) to a store's fluid from outside it (heat
    /// through its walls, a reaction's product), booked as brought in.
    pub fn add(&mut self, node: usize, q: usize, amount: f64) {
        let p = &mut self.points[node];
        assert!(
            p.holding == Holding::Store && p.mass > 0.0,
            "only a store's fluid takes it"
        );
        p.content[q] += amount / p.mass;
        self.ledgers[q + 1].import(amount);
    }

    /// A node's content now (a junction's: what it last passed on).
    pub fn content(&self, node: usize) -> &[f64] {
        &self.points[node].content
    }

    /// What a link delivers at its end `to` (a node of it): its parcel there (a link holding
    /// nothing: its upstream node's content).
    pub fn delivered(&self, link: usize, to: usize) -> Vec<f64> {
        let line = &self.lines[link];
        let n = self.k + 1;
        if line.holdup <= 0.0 {
            let from = if to == line.b { line.a } else { line.b };
            return self.points[from].content.clone();
        }
        let len = line.parcels.len();
        let start = if to == line.b { len - n + 1 } else { 1 };
        line.parcels.range(start..start + self.k).copied().collect()
    }

    /// The mass and each quantity it holds (kg; the quantities' units times kilograms).
    pub fn held(&self) -> Vec<f64> {
        let mut held = vec![Sum::default(); self.k + 1];
        let mut add = |mass: f64, content: &[f64]| {
            held[0].add(mass);
            for (h, c) in held[1..].iter_mut().zip(content) {
                h.add(mass * c);
            }
        };
        for p in self.points.iter().filter(|p| p.holding == Holding::Store) {
            add(p.mass, &p.content);
        }
        let n = self.k + 1;
        for line in &self.lines {
            for i in (0..line.parcels.len()).step_by(n) {
                let parcel: Vec<f64> = line.parcels.range(i..i + n).copied().collect();
                add(parcel[0], &parcel[1..]);
            }
        }
        held.iter().map(Sum::value).collect()
    }

    /// The ledger of the mass (`0`) or of quantity `q − 1`.
    pub fn ledger(&self, q: usize) -> &Ledger {
        &self.ledgers[q]
    }

    #[cfg(test)]
    fn parcels_of(&self, link: usize) -> usize {
        self.lines[link].parcels.len() / (self.k + 1)
    }

    /// A store's mass (kg).
    pub fn mass(&self, node: usize) -> f64 {
        self.points[node].mass
    }
}

/// Whether two quantities are one parcel's.
fn same(a: f64, b: f64) -> bool {
    (a - b).abs() <= SAME * a.abs().max(b.abs())
}
