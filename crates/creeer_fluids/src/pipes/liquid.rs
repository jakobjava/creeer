//! A liquid network's step: the global gradient method.
//!
//! Each open link's law, linearised at its flow, H_a − H_b ≈ h(Q₀) + y·(Q − Q₀), gives its
//! flow as c·(H_a − H_b) + q₀ (c = 1/y, q₀ = Q₀ − h(Q₀)/y); put into each junction's and
//! tank's balance (in − out = demand + A·(H − H_old)/Δt) they make a symmetric positive
//! definite system in the heads alone. Its solution gives new flows, its law is linearised
//! there, until the flows stop moving and no one-way link wants to switch.

use super::links::{efficiency, law, one_way, passable};
use super::{LinkKind, Network, NodeKind, PipeReport};
use creeer_math::sparse::{Assembly, Cholesky};
use creeer_network::Sum;

/// Newton iterations at most per step.
const ITERATIONS: usize = 60;

/// The solve's structure: which nodes are unknowns, the matrix pattern, the factor.
#[derive(Clone, Debug)]
pub(crate) struct Solver {
    unknown: Vec<Option<usize>>,
    assembly: Assembly,
    cholesky: Option<Cholesky>,
}

impl Solver {
    fn new(net: &Network) -> Self {
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
        let old: Vec<f64> = self.nodes.iter().map(|n| n.head).collect();
        let (liquid, g) = (self.liquid, self.gravity);
        let mut report = PipeReport::default();
        let mut contributions = Vec::new();
        let mut rhs = vec![0.0; n];
        for _ in 0..ITERATIONS {
            report.iterations += 1;
            contributions.clear();
            rhs.fill(0.0);
            let mut linear = Vec::with_capacity(self.links.len());
            for link in &self.links {
                let (c, q0) = if link.open && passable(link) {
                    let (h, y) = law(link, &liquid, g, link.flow);
                    (1.0 / y, link.flow - h / y)
                } else {
                    (0.0, 0.0)
                };
                linear.push((c, q0));
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
                // (A junction all of whose links are shut has no head: a touch on its
                // diagonal keeps the system regular.)
                contributions.push(diag + 1e-12);
                rhs[r] += extra;
            }
            let matrix = solver.assembly.matrix(&contributions);
            let factored = match solver.cholesky.as_mut() {
                Some(ch) => ch.refactor(&matrix).map(|()| ()),
                None => Cholesky::new(&matrix).map(|ch| solver.cholesky = Some(ch)),
            };
            factored.expect("a pipe network's system is positive definite");
            solver
                .cholesky
                .as_mut()
                .expect("factorised")
                .solve(&mut rhs);
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
            if !switched && moved <= 1e-12 + 1e-9 * largest {
                report.converged = true;
                break;
            }
        }
        self.book(&old, dt);
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
        for link in self.links.iter().filter(|l| l.open && passable(l)) {
            net[link.a] -= link.flow;
            net[link.b] += link.flow;
            let (h, _) = law(link, &self.liquid, self.gravity, link.flow);
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
