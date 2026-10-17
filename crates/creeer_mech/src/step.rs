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
use crate::elements;
use crate::solve::Factored;
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

/// One round's system: its matrix and right-hand side, each element's multiplier column.
struct System {
    n: usize,
    a: Vec<f64>,
    b: Vec<f64>,
    column: Vec<Option<usize>>,
}

impl Drivetrain {
    fn index(node: usize) -> Option<usize> {
        (node != HOUSING).then(|| node - 1)
    }

    /// The system for step `h` with grips `grip` and power directions `forward`.
    fn assemble(&self, h: f64, grip: &[Grip], forward: &[bool]) -> System {
        let parts = self.parts.len() - 1;
        let mut column = vec![None; self.elements.len()];
        let mut n = parts;
        for (k, e) in self.elements.iter().enumerate() {
            if elements::is_rigid(e, grip[k]) {
                column[k] = Some(n);
                n += 1;
            }
        }
        let mut a = vec![0.0; n * n];
        let mut b = vec![0.0; n];
        for (i, p) in self.parts.iter().enumerate().skip(1) {
            let r = i - 1;
            a[r * n + r] = p.inertia / h;
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
                                if let Some(o) = Self::index(other) {
                                    a[r * n + o] += sign * s * g;
                                }
                            }
                        }
                    }
                    continue;
                }
                Element::Bearing { a: na, viscous, .. } => {
                    if let Some(r) = Self::index(na) {
                        a[r * n + r] += viscous;
                    }
                }
                _ => {}
            }
            match column[k] {
                Some(c) => {
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
                None => {
                    for (node, t) in elements::sliding(e, grip[k], forward[k]) {
                        if let Some(r) = Self::index(node) {
                            b[r] += t;
                        }
                    }
                }
            }
        }
        System { n, a, b, column }
    }

    /// Solves `system`: the parts' speeds then the multipliers.
    fn solve_system(system: System, parts: usize) -> Option<(Vec<f64>, System)> {
        let x = Factored::new(&system.a, system.n, parts)?.solve(&system.b);
        Some((x, system))
    }

    /// The speed a node would end a step of `h` with (rad/s), and how much faster per N·m
    /// more on it (rad/s per N·m): what an electric machine on it sees for the step (its
    /// shaft solved jointly with its circuit, ADR-0012).
    ///
    /// # Panics
    /// If the drivetrain's system is singular (a part without inertia).
    pub fn response(&self, node: usize, h: f64) -> (f64, f64) {
        let forward = self.forwards();
        let index = Self::index(node).expect("a part, not the housing");
        let system = self.assemble(h, &self.grip, &forward);
        let factored = Factored::new(&system.a, system.n, self.parts.len() - 1)
            .expect("a drivetrain's system is regular");
        let free = factored.solve(&system.b)[index];
        // The system is linear: one N·m more on the node alone, solved.
        let mut unit = vec![0.0; system.n];
        unit[index] = 1.0;
        (free, factored.solve(&unit)[index])
    }

    /// Whether power flowed forward through each transmission last step.
    fn forwards(&self) -> Vec<bool> {
        self.elements
            .iter()
            .zip(&self.carried)
            .map(|(e, &lambda)| elements::flows_forward(e, lambda, |node| self.parts[node].speed))
            .collect()
    }

    /// Advances it by `h` seconds, its housing turning at `spin` (rad/s, in its own frame:
    /// for the gyroscopic reaction).
    ///
    /// # Panics
    /// If its system is singular (a part without inertia).
    pub fn step(&mut self, h: f64, spin: DVec3) -> MechReport {
        let mut grip = self.grip.clone();
        let mut forward = self.forwards();
        let mut report = MechReport::default();
        let (x, system) = loop {
            report.rounds += 1;
            let (x, system) =
                Self::solve_system(self.assemble(h, &grip, &forward), self.parts.len() - 1)
                    .expect("a drivetrain's system is regular");
            let changed = crate::grip::update(self, h, &x, &system.column, &mut grip, &mut forward);
            if !changed || report.rounds == ROUNDS {
                report.settled = !changed;
                break (x, system);
            }
        };
        crate::book::finish(self, h, spin, &x, &system.column, &grip, &forward);
        self.grip = grip;
        report
    }
}

/// What `book` and `grip` need of a round's solution.
pub(crate) fn speed(x: &[f64], node: usize) -> f64 {
    if node == HOUSING { 0.0 } else { x[node - 1] }
}
