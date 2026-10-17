//! A drivetrain's step: backward Euler on its parts' speeds, its rigid constraints (gears,
//! gripping belts, stuck clutches and brakes) as Lagrange multipliers, its friction resolved
//! by an active set (each friction element stuck or slipping, switched until consistent: no
//! chattering), solved through its constraints' Schur complement (`solve`).
//!
//! **Energy.** The parts' equations J·(ω − ω₀)/h = ΣT, multiplied by their midpoint speeds
//! (ω + ω₀)/2 and summed, say ΔKE = h·Σ(element power at the midpoint), exactly: each element
//! books its own (a shaft its strain energy's change and its damping, a gear its loss in the
//! direction power flowed, a slipping clutch its friction heat, a torque from outside its
//! work), and the ledger closes to the solve's rounding.

use crate::drivetrain::{Drivetrain, Element, Grip, HOUSING};
use crate::elements::{self, Flow};
use crate::solve::Scratch;
use creeer_math::glam::DVec3;

/// Active-set rounds at most (each switches at least one friction element).
const ROUNDS: usize = 30;

/// What a step did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MechReport {
    /// Active-set rounds, and whether they settled.
    pub rounds: usize,
    pub settled: bool,
}

impl Drivetrain {
    fn index(node: usize) -> Option<usize> {
        (node != HOUSING).then(|| node - 1)
    }

    /// The system for step `h` with grips `grip` and power directions `forward`, into `s`:
    /// its right-hand side, and its matrix too if `matrix` (else the one factorised stands).
    fn assemble(&self, h: f64, grip: &[Grip], forward: &[Flow], s: &mut Scratch, matrix: bool) {
        let parts = self.parts.len() - 1;
        let rigid = (self.elements.iter().zip(grip))
            .filter(|(e, g)| elements::is_rigid(e, **g))
            .count();
        let n = parts + rigid;
        s.reset(n, parts, self.elements.len(), matrix);
        let mut next = parts;
        for (k, e) in self.elements.iter().enumerate() {
            if elements::is_rigid(e, grip[k]) {
                s.column[k] = Some(next);
                next += 1;
            }
        }
        let (a, b, column) = (&mut s.a, &mut s.b, &s.column);
        for (i, p) in self.parts.iter().enumerate().skip(1) {
            let r = i - 1;
            if matrix {
                a[r * n + r] = p.inertia / h;
            }
            b[r] = p.inertia * p.speed / h + p.torque + p.load;
        }
        for (k, e) in self.elements.iter().enumerate() {
            match *e {
                Element::Shaft {
                    a: na,
                    b: nb,
                    stiffness,
                    damping,
                } => {
                    // Torque on a: −(k·(φ₀ + h·(ω_a − ω_b)) + c·(ω_a − ω_b)), on b the opposite.
                    let g = stiffness * h + damping;
                    let phi = self.twist[k];
                    for (node, sign) in [(na, 1.0), (nb, -1.0)] {
                        if let Some(r) = Self::index(node) {
                            b[r] -= sign * stiffness * phi;
                            for (other, s) in [(na, 1.0), (nb, -1.0)] {
                                if let Some(o) = Self::index(other).filter(|_| matrix) {
                                    a[r * n + o] += sign * s * g;
                                }
                            }
                        }
                    }
                    continue;
                }
                Element::Bearing { a: na, viscous, .. } => {
                    if let Some(r) = Self::index(na).filter(|_| matrix) {
                        a[r * n + r] += viscous;
                    }
                }
                _ => {}
            }
            match column[k] {
                Some(c) if matrix => {
                    let (row, torque) = elements::rigid(e, forward[k]);
                    for (node, g) in row {
                        if let Some(r) = Self::index(node) {
                            a[c * n + r] += g;
                        }
                    }
                    // Its torque t·λ on a node, moved to the left.
                    for (node, t) in torque {
                        if let Some(r) = Self::index(node) {
                            a[r * n + c] -= t;
                        }
                    }
                }
                Some(_) => {}
                None => {
                    for (node, t) in elements::sliding(e, grip[k], forward[k]) {
                        if let Some(r) = Self::index(node) {
                            b[r] += t;
                        }
                    }
                }
            }
        }
    }

    /// Assembles the round's system in `w` for its grips and flows, refactorising only if its
    /// matrix changed: the same step, grips, flows and build give the very same matrix (a
    /// drivetrain running steadily refactorised it every step: half its time).
    fn prepare(&self, h: f64, w: &mut Work) {
        #[allow(clippy::float_cmp)] // the very same step
        let same = w.key.as_ref().is_some_and(|k| {
            k.h == h && k.version == self.version && k.grip == w.grip && k.flow == w.flow
        });
        self.assemble(h, &w.grip, &w.flow, &mut w.scratch, !same);
        if !same {
            assert!(w.scratch.factor(), "a drivetrain's system is regular");
            let key = w.key.get_or_insert_with(Key::default);
            (key.h, key.version) = (h, self.version);
            key.grip.clone_from(&w.grip);
            key.flow.clone_from(&w.flow);
        }
    }

    /// The speed a node would end a step of `h` with (rad/s), and how much faster per N·m
    /// more on it (rad/s per N·m): what an electric machine on it sees for the step (its
    /// shaft solved jointly with its circuit, ADR-0012).
    ///
    /// # Panics
    /// If the drivetrain's system is singular (a part without inertia).
    pub fn response(&mut self, node: usize, h: f64) -> (f64, f64) {
        let index = Self::index(node).expect("a part, not the housing");
        let mut w = std::mem::take(&mut self.work);
        self.forwards(h, &mut w.flow);
        w.grip.clone_from(&self.grip);
        self.prepare(h, &mut w);
        let b = std::mem::take(&mut w.scratch.b);
        w.scratch.solve(&b, &mut w.x);
        let free = w.x[index];
        // The system is linear: one N·m more on the node alone, solved.
        w.unit.clear();
        w.unit.resize(b.len(), 0.0);
        w.unit[index] = 1.0;
        let unit = std::mem::take(&mut w.unit);
        w.scratch.solve(&unit, &mut w.x);
        let mobility = w.x[index];
        (w.unit, w.scratch.b) = (unit, b);
        self.work = w;
        (free, mobility)
    }

    /// Which way power flows through each transmission as a step starts, into `flow`: as it
    /// did, but for one that went neutral (lossless) last step, decided anew from what it
    /// carried.
    fn forwards(&self, h: f64, flow: &mut Vec<Flow>) {
        let noise = crate::grip::noise(self, &self.carried, h);
        flow.clear();
        flow.extend(self.elements.iter().zip(&self.carried).zip(&self.flow).map(
            |((e, &carried), &f)| {
                match f {
                    Flow::Neutral => {
                        elements::flow(e, carried, |node| self.parts[node].speed, noise)
                            .unwrap_or(Flow::Neutral)
                    }
                    _ => f,
                }
            },
        ));
    }

    /// Advances it by `h` seconds, its housing turning at `spin` (rad/s, in its own frame:
    /// for the gyroscopic reaction).
    ///
    /// # Panics
    /// If its system is singular (a part without inertia).
    pub fn step(&mut self, h: f64, spin: DVec3) -> MechReport {
        let mut w = std::mem::take(&mut self.work);
        w.grip.clone_from(&self.grip);
        self.forwards(h, &mut w.flow);
        w.flips.clear();
        w.flips.resize(self.elements.len(), 0);
        let mut report = MechReport::default();
        loop {
            report.rounds += 1;
            self.prepare(h, &mut w);
            let b = std::mem::take(&mut w.scratch.b);
            w.scratch.solve(&b, &mut w.x);
            w.scratch.b = b;
            w.held.clone_from(&w.grip);
            w.flowed.clone_from(&w.flow);
            let column = &w.scratch.column;
            let changed = crate::grip::update(
                self,
                h,
                &w.x,
                column,
                &mut w.grip,
                &mut w.flow,
                &mut w.flips,
            );
            if !changed || report.rounds == ROUNDS {
                report.settled = !changed;
                if changed {
                    // Unsettled, the step ends as its last round solved it (its books must
                    // be of the torques that solve had).
                    std::mem::swap(&mut w.grip, &mut w.held);
                    std::mem::swap(&mut w.flow, &mut w.flowed);
                }
                break;
            }
        }
        crate::book::finish(self, h, spin, &w.x, &w.scratch.column, &w.grip, &w.flow);
        self.grip.clone_from(&w.grip);
        self.flow.clone_from(&w.flow);
        self.work = w;
        report
    }
}

/// What `book` and `grip` need of a round's solution.
pub(crate) fn speed(x: &[f64], node: usize) -> f64 {
    if node == HOUSING { 0.0 } else { x[node - 1] }
}

/// What a step works in, kept from step to step: its system, its solution, its grips and
/// flows as the rounds change them.
/// What a factorised matrix was built for.
#[derive(Clone, Debug, Default)]
struct Key {
    h: f64,
    version: u64,
    grip: Vec<Grip>,
    flow: Vec<Flow>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Work {
    scratch: Scratch,
    key: Option<Key>,
    x: Vec<f64>,
    unit: Vec<f64>,
    grip: Vec<Grip>,
    flow: Vec<Flow>,
    held: Vec<Grip>,
    flowed: Vec<Flow>,
    flips: Vec<u8>,
}
