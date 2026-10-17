//! A liquid network's step: the global gradient method.
//!
//! Each open link's law, linearised at its flow, H_a − H_b ≈ h(Q₀) + y·(Q − Q₀), gives its
//! flow as c·(H_a − H_b) + q₀ (c = 1/y, q₀ = Q₀ − h(Q₀)/y); put into each junction's and
//! tank's balance (in − out = demand + A·(H − H_old)/Δt) they make a symmetric positive
//! definite system in the heads alone. Its solution gives new flows, its law is linearised
//! there, until the flows stop moving and no one-way link wants to switch.

use super::friction::Hint;
use super::links::{efficiency, law, law_from, one_way, passable};
use super::{LinkKind, Network, NodeKind, PipeReport};
use creeer_math::sparse::{Assembly, Cholesky, Csc};
// (`Cholesky` kept between steps: `solve.rs` preconditions with it.)
use creeer_network::Sum;
use rayon::prelude::*;

/// Newton iterations at most per step.
const ITERATIONS: usize = 60;

/// Links from which a step evaluates its laws in parallel (below, the tasks cost more).
const PARALLEL: usize = 256;

/// The solve's structure: which nodes are unknowns, the matrix pattern, the factor.
#[derive(Clone, Debug)]
pub(crate) struct Solver {
    unknown: Vec<Option<usize>>,
    assembly: Assembly,
    cholesky: Option<Cholesky>,
    /// Each link's friction factor as 1/√f, where its last solve found it.
    hints: Vec<Hint>,
    /// Newton's contraction: its next move is about this times its last squared (1/(m³/s)).
    rate: f64,
    /// A step's buffers, kept.
    old: Vec<f64>,
    contributions: Vec<f64>,
    rhs: Vec<f64>,
    linear: Vec<(f64, f64)>,
    matrix: Option<Csc<f64>>,
    /// Which links were open, and which nodes that let reach a reservoir or a tank.
    reach: (Vec<bool>, Vec<bool>),
}

impl Solver {
    pub(crate) fn new(net: &Network) -> Self {
        let mut unknown = vec![None; net.nodes.len()];
        let mut n = 0;
        for (i, node) in net.nodes.iter().enumerate() {
            if !matches!(node.kind, NodeKind::Reservoir) {
                unknown[i] = Some(n);
                n += 1;
            }
        }
        let mut positions = Vec::new();
        for link in &net.links {
            let (a, b) = (unknown[link.a], unknown[link.b]);
            for (r, c) in [(a, a), (b, b), (a, b), (b, a)] {
                if let (Some(r), Some(c)) = (r, c) {
                    positions.push((r, c));
                }
            }
        }
        positions.extend((0..n).map(|i| (i, i)));
        Self {
            unknown,
            assembly: Assembly::new(n, &positions),
            cholesky: None,
            hints: Vec::new(),
            rate: f64::INFINITY,
            old: Vec::new(),
            contributions: Vec::new(),
            rhs: Vec::new(),
            linear: Vec::new(),
            matrix: None,
            reach: (Vec::new(), Vec::new()),
        }
    }
}

impl Network {
    /// Advances the network by `dt` seconds: its flows to the heads now, its tanks filling
    /// or emptying. Returns how Newton went.
    ///
    /// # Panics
    /// If its system is not positive definite (impossible: every law's slope is positive).
    pub fn step(&mut self, dt: f64) -> PipeReport {
        let mut solver = self.solver.take().unwrap_or_else(|| Solver::new(self));
        let n = solver.assembly.n();
        // (Buffers kept between steps: a small network's step is a few microseconds, and its
        // allocations were half of them.)
        let mut old = std::mem::take(&mut solver.old);
        old.clear();
        old.extend(self.nodes.iter().map(|n| n.head));
        let (liquid, g) = (self.liquid, self.gravity);
        let mut report = PipeReport::default();
        let mut contributions = std::mem::take(&mut solver.contributions);
        let mut rhs = std::mem::take(&mut solver.rhs);
        rhs.resize(n, 0.0);
        let mut linear = std::mem::take(&mut solver.linear);
        let mut matrix = solver
            .matrix
            .take()
            .unwrap_or_else(|| solver.assembly.zeros());
        // Which junctions reach a reservoir or a tank (again only when a link opens or shuts).
        let mut held: Option<Vec<bool>> = None;
        let mut reach = std::mem::take(&mut solver.reach);
        let mut before = f64::INFINITY;
        for _ in 0..ITERATIONS {
            report.iterations += 1;
            contributions.clear();
            rhs.fill(0.0);
            // Each link's law linearised (in parallel for a large network: the friction factors
            // are most of its step).
            solver.hints.resize(self.links.len(), Hint::default());
            let linearise = |(link, hint): (&super::Link, &mut Hint)| {
                if link.open && passable(link) {
                    let (h, y) = law_from(link, &liquid, g, link.flow, hint);
                    (1.0 / y, link.flow - h / y)
                } else {
                    (0.0, 0.0)
                }
            };
            if self.links.len() >= PARALLEL {
                (self
                    .links
                    .par_iter()
                    .zip(solver.hints.par_iter_mut())
                    .with_min_len(PARALLEL))
                .map(linearise)
                .collect_into_vec(&mut linear);
            } else {
                linear.clear();
                linear.extend(
                    self.links
                        .iter()
                        .zip(solver.hints.iter_mut())
                        .map(linearise),
                );
            }
            if held.is_none() {
                // Kept from the last step while the same links are open (it rarely changes).
                let same = reach.0.len() == linear.len()
                    && reach.0.iter().zip(&linear).all(|(o, l)| *o == (l.0 > 0.0));
                if !same {
                    reach.0.clear();
                    reach.0.extend(linear.iter().map(|l| l.0 > 0.0));
                    reach.1 = grounded(self, &linear);
                }
                held = Some(std::mem::take(&mut reach.1));
            }
            let reached = held.as_ref().expect("found");
            for (link, &(c, q0)) in self.links.iter().zip(&linear) {
                let (a, b) = (solver.unknown[link.a], solver.unknown[link.b]);
                // Q = c·(H_a − H_b) + q₀ leaves a and enters b.
                for (r, col, v) in [(a, a, c), (b, b, c), (a, b, -c), (b, a, -c)] {
                    if let (Some(_), Some(_)) = (r, col) {
                        contributions.push(v);
                    }
                }
                if let Some(a) = a {
                    rhs[a] -= q0;
                    if b.is_none() {
                        rhs[a] += c * self.nodes[link.b].head;
                    }
                }
                if let Some(b) = b {
                    rhs[b] += q0;
                    if a.is_none() {
                        rhs[b] += c * self.nodes[link.a].head;
                    }
                }
            }
            for (i, node) in self.nodes.iter().enumerate() {
                let Some(r) = solver.unknown[i] else { continue };
                let (diag, extra) = match node.kind {
                    NodeKind::Tank { area, .. } => (area / dt, area / dt * old[i]),
                    NodeKind::Junction { demand } => (0.0, -demand),
                    NodeKind::Reservoir => unreachable!("a reservoir is known"),
                };
                // (Junctions cut off from every reservoir and tank have no head: a touch on
                // their diagonals keeps the system regular. Only there: elsewhere it would
                // leak 10⁻¹² m³/s per metre of head.)
                contributions.push(if reached[i] { diag } else { diag + 1e-12 });
                rhs[r] += extra;
            }
            solver.assembly.fill(&contributions, &mut matrix.values);
            // A step's first iteration meets laws moved since the last step: factorised afresh;
            // its later ones, laws moved a little: the factor preconditions them (`solve.rs`).
            super::solve::solve(
                &mut solver.cholesky,
                &matrix,
                &mut rhs,
                report.iterations == 1,
            )
            .expect("a pipe network's system is positive definite");
            for (i, node) in self.nodes.iter_mut().enumerate() {
                if let Some(r) = solver.unknown[i] {
                    node.head = rhs[r];
                }
            }
            let mut moved = 0.0f64;
            let mut largest = 0.0f64;
            let mut switched = false;
            for (link, &(c, q0)) in self.links.iter_mut().zip(&linear) {
                let drive = self.nodes[link.a].head - self.nodes[link.b].head;
                let q = c * drive + q0;
                moved = moved.max((q - link.flow).abs());
                largest = largest.max(q.abs());
                link.flow = q;
                if one_way(link) {
                    if link.open && q < 0.0 {
                        (link.open, link.flow) = (false, 0.0);
                        switched = true;
                    } else if !link.open {
                        // Shut: would it push forward? A check valve if the head ahead is
                        // lower; a pump if its shut-off head beats the lift.
                        let push = match link.kind {
                            LinkKind::Pump { shutoff, speed, .. } => {
                                speed * speed * shutoff + drive
                            }
                            _ => drive,
                        };
                        if push > 0.0 {
                            link.open = true;
                            switched = true;
                        }
                    }
                }
            }
            // Converged when the flows stopped moving, or are converging quadratically (as
            // Newton's method does near its root) to where the next move would be below that:
            // the flows taken are then as good as one more iteration's.
            let tolerance = 1e-12 + 1e-9 * largest;
            let quadratic =
                before.is_finite() && moved <= 0.1 * before && moved * moved / before <= tolerance;
            // Newton's next move is C·(this one)²: C as the last pair of moves measured it (this
            // step's, or the last step's for its first iteration: the laws change slowly from step
            // to step), with a tenfold margin. (Deuflhard's contraction estimate.)
            if before.is_finite() && before > 0.0 && moved <= 0.1 * before {
                solver.rate = moved / (before * before);
            }
            let predicted = !before.is_finite() && 10.0 * solver.rate * moved * moved <= tolerance;
            if !switched && (moved <= tolerance || quadratic || predicted) {
                report.converged = true;
                break;
            }
            before = moved;
            if switched {
                if let Some(h) = held.take() {
                    reach.1 = h;
                }
                before = f64::INFINITY;
            }
        }
        if let Some(h) = held {
            reach.1 = h;
        }
        solver.reach = reach;
        self.book(&old, dt);
        (solver.old, solver.contributions, solver.rhs, solver.linear) =
            (old, contributions, rhs, linear);
        solver.matrix = Some(matrix);
        self.solver = Some(solver);
        report
    }

    /// The step's energy: pumps' shaft work in, friction and the pumps' losses as heat,
    /// water's ρgHQ into and out of reservoirs and consumers, the tanks' potential energy
    /// (backward Euler's ρgA·ΔH²/2 of it to heat).
    fn book(&mut self, old: &[f64], dt: f64) {
        let rg = self.liquid.density * self.gravity;
        let (mut imported, mut exported, mut dissipated) =
            (Sum::default(), Sum::default(), Sum::default());
        let mut net = vec![0.0; self.nodes.len()];
        // Each link's head loss is its law's as the last solve linearised it: exactly the heads'
        // difference the flows were found for. So the books close to rounding whatever Newton's
        // residual (below its tolerance), and no law is evaluated again (it was a third of a
        // small network's step).
        for link in self.links.iter().filter(|l| l.open && passable(l)) {
            net[link.a] -= link.flow;
            net[link.b] += link.flow;
            let h = self.nodes[link.a].head - self.nodes[link.b].head;
            if let LinkKind::Pump { .. } = link.kind {
                let hydraulic = -rg * link.flow * h;
                let shaft = hydraulic / efficiency(&link.kind, link.flow);
                imported.add(shaft * dt);
                dissipated.add((shaft - hydraulic) * dt);
            } else {
                dissipated.add(rg * link.flow * h * dt);
            }
        }
        for (i, node) in self.nodes.iter().enumerate() {
            match node.kind {
                NodeKind::Reservoir => {
                    let out = -net[i] * rg * node.head * dt;
                    if out >= 0.0 {
                        imported.add(out)
                    } else {
                        exported.add(-out)
                    }
                }
                NodeKind::Junction { demand } => exported.add(demand * rg * node.head * dt),
                NodeKind::Tank { area, .. } => {
                    let change = node.head - old[i];
                    dissipated.add(0.5 * rg * area * change * change);
                }
            }
        }
        self.ledger.import(imported.value());
        self.ledger.export(exported.value());
        self.ledger.dissipate(dissipated.value());
    }

    /// The power a pump draws from its shaft now (W): ρ·g·Q·H/η.
    pub fn shaft_power(&self, link: usize) -> f64 {
        let l = &self.links[link];
        if !l.open {
            return 0.0;
        }
        let (h, _) = law(l, &self.liquid, self.gravity, l.flow);
        -self.liquid.density * self.gravity * l.flow * h / efficiency(&l.kind, l.flow)
    }
}

/// Whether each node is joined by open links to a reservoir or a tank (one that holds the
/// heads of what it is joined to).
fn grounded(net: &Network, linear: &[(f64, f64)]) -> Vec<bool> {
    let mut root: Vec<usize> = (0..net.nodes.len()).collect();
    fn find(root: &mut [usize], mut i: usize) -> usize {
        while root[i] != i {
            root[i] = root[root[i]];
            i = root[i];
        }
        i
    }
    for (link, &(c, _)) in net.links.iter().zip(linear) {
        if c > 0.0 {
            let (a, b) = (find(&mut root, link.a), find(&mut root, link.b));
            root[a] = b;
        }
    }
    let mut held = vec![false; net.nodes.len()];
    for (i, node) in net.nodes.iter().enumerate() {
        if !matches!(node.kind, NodeKind::Junction { .. }) {
            let r = find(&mut root, i);
            held[r] = true;
        }
    }
    (0..net.nodes.len())
        .map(|i| held[find(&mut root, i)])
        .collect()
}

impl Solver {
    /// The factorisation's size for this pattern, found on the pattern itself (its graph's
    /// Laplacian plus the identity: positive definite, the same fill): the entries of L and a
    /// factorisation's multiply-adds. Kept as the factor the first step refactorises.
    pub(crate) fn pattern_size(&mut self, net: &Network) -> (usize, usize) {
        let mut contributions = Vec::new();
        let diagonal = vec![1.0; self.assembly.n()];
        for link in &net.links {
            let (a, b) = (self.unknown[link.a], self.unknown[link.b]);
            for (r, c, v) in [(a, a, 1.0), (b, b, 1.0), (a, b, -1.0), (b, a, -1.0)] {
                if let (Some(_), Some(_)) = (r, c) {
                    contributions.push(v);
                }
            }
        }
        contributions.extend(diagonal);
        let matrix = self.assembly.matrix(&contributions);
        let factor =
            Cholesky::new(&matrix).expect("a Laplacian plus the identity is positive definite");
        let size = (factor.nnz(), factor.flops());
        self.cholesky = Some(factor);
        size
    }
}
