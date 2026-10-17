//! Gas pipe networks: isothermal compressible flow (a long pipe's gas takes its ground's
//! temperature). Each pipe: p_a² − p_b² = (R·T/M)·f·(L/D)·(ṁ/A)² (the isothermal flow
//! equation without its kinetic term, which a pipeline's length makes negligible; as Weymouth
//! and Panhandle take it), f by Colebrook at Re = (ṁ/A)·D/μ, which for a gas does not depend
//! on the pressure. Nodes store gas, V·M/(R·T)·dp/dt = Σ ṁ_in − Σ ṁ_out − demand, by backward
//! Euler; Newton's method on the pressures with the sparse LU (the Jacobian is not symmetric).
//! Its ledger counts mass (kg): supplies in, demands out, the line pack held.

use super::friction::darcy;
use creeer_materials::species::R;
use creeer_math::sparse::{Assembly, Lu};
use creeer_network::Ledger;

/// A gas: molar mass (kg/mol), viscosity (Pa·s), temperature (K).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gas {
    pub molar_mass: f64,
    pub viscosity: f64,
    pub temperature: f64,
}

impl Gas {
    /// Pipeline natural gas as methane at 15 °C (μ 11.0 µPa·s).
    pub const METHANE_15C: Self = Self {
        molar_mass: 0.016_043,
        viscosity: 1.10e-5,
        temperature: 288.15,
    };
}

/// What a gas node is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GasKind {
    /// Its pressure held (a field, a compressor station's outlet, a city gate).
    Supply,
    /// A volume (m³: its own and its pipes' share) that stores gas, `demand` kg/s drawn off.
    Store { volume: f64, demand: f64 },
}

/// A gas node and its pressure (Pa).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GasNode {
    pub kind: GasKind,
    pub pressure: f64,
}

/// A gas pipe from `a` to `b`: length, diameter, roughness (m), a fixed Darcy factor if one is
/// given (Weymouth's), its mass flow (kg/s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GasPipe {
    pub a: usize,
    pub b: usize,
    pub length: f64,
    pub diameter: f64,
    pub roughness: f64,
    pub friction: Option<f64>,
    pub flow: f64,
}

/// The solve's structure: which nodes are unknowns, the Jacobian's pattern, its factor.
#[derive(Clone, Debug)]
struct Structure {
    unknown: Vec<Option<usize>>,
    assembly: Assembly,
    lu: Option<Lu<f64>>,
}

/// A gas network.
#[derive(Clone, Debug)]
pub struct GasNetwork {
    pub nodes: Vec<GasNode>,
    pub pipes: Vec<GasPipe>,
    pub gas: Gas,
    ledger: Ledger,
    structure: Option<Structure>,
}

impl GasPipe {
    /// Its flow (kg/s) between pressures `pa` and `pb`, and the flow's derivatives by them.
    fn flow(&self, gas: &Gas, pa: f64, pb: f64) -> (f64, f64, f64) {
        let area = 0.25 * std::f64::consts::PI * self.diameter * self.diameter;
        let k = area * area * self.diameter * gas.molar_mass / (R * gas.temperature * self.length);
        // ṁ|ṁ|·f(ṁ) = S.
        let s = (pa * pa - pb * pb) * k;
        // A fixed factor (Weymouth's) is a turbulent flow's: near no flow the gas is laminar,
        // 64/Re (taken as fixed there, its law had no slope at nought: Newton's steps vanished
        // and read as converged).
        let f = |m: f64| {
            let re = m.abs() / area * self.diameter / gas.viscosity;
            match self.friction {
                Some(f) => f.max(64.0 / re.max(1e-300)),
                None => darcy(re, self.roughness / self.diameter),
            }
        };
        let g = |m: f64| m * m.abs() * f(m) - s;
        // Newton on ṁ from the turbulent guess (laminar where the friction law makes it so).
        let mut m = s.signum() * (s.abs() / f(1.0).max(1e-3)).sqrt();
        for _ in 0..40 {
            let d = 1e-7 * m.abs().max(1e-9);
            let slope = (g(m + d) - g(m - d)) / (2.0 * d);
            let step = g(m) / slope.max(1e-300);
            m -= step;
            if step.abs() <= 1e-13 * m.abs().max(1e-12) {
                break;
            }
        }
        let d = 1e-7 * m.abs().max(1e-9);
        let dm_ds = 2.0 * d / (g(m + d) - g(m - d)).max(1e-300);
        (m, dm_ds * 2.0 * pa * k, -dm_ds * 2.0 * pb * k)
    }
}

impl GasNetwork {
    /// An empty network of `gas`.
    pub fn new(gas: Gas) -> Self {
        Self {
            nodes: Vec::new(),
            pipes: Vec::new(),
            gas,
            ledger: Ledger::new(0.0),
            structure: None,
        }
    }

    /// Adds a node at `pressure` (Pa).
    pub fn node(&mut self, kind: GasKind, pressure: f64) -> usize {
        let held = self.stored();
        self.nodes.push(GasNode { kind, pressure });
        self.ledger.joined(self.stored() - held);
        self.structure = None;
        self.nodes.len() - 1
    }

    /// Adds a pipe from `a` to `b`.
    pub fn pipe(
        &mut self,
        a: usize,
        b: usize,
        length: f64,
        diameter: f64,
        roughness: f64,
        friction: Option<f64>,
    ) -> usize {
        self.pipes.push(GasPipe {
            a,
            b,
            length,
            diameter,
            roughness,
            friction,
            flow: 0.0,
        });
        self.structure = None;
        self.pipes.len() - 1
    }

    /// The gas it holds (kg): its stores' line pack, p·V·M/(R·T).
    pub fn stored(&self) -> f64 {
        let c = self.gas.molar_mass / (R * self.gas.temperature);
        self.nodes
            .iter()
            .map(|n| {
                if let GasKind::Store { volume, .. } = n.kind {
                    c * volume * n.pressure
                } else {
                    0.0
                }
            })
            .sum()
    }

    /// Where its mass went (kg).
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Advances it by `dt` seconds; whether Newton converged.
    ///
    /// # Panics
    /// If its Jacobian is singular (a store with no volume and no pipe).
    pub fn step(&mut self, dt: f64) -> bool {
        let Structure {
            unknown,
            assembly,
            mut lu,
        } = self.structure.take().unwrap_or_else(|| {
            let mut unknown = vec![None; self.nodes.len()];
            let mut n = 0;
            for (i, node) in self.nodes.iter().enumerate() {
                if let GasKind::Store { .. } = node.kind {
                    unknown[i] = Some(n);
                    n += 1;
                }
            }
            let mut positions: Vec<(usize, usize)> = (0..n).map(|i| (i, i)).collect();
            for p in &self.pipes {
                for (r, c) in [(p.a, p.a), (p.a, p.b), (p.b, p.a), (p.b, p.b)] {
                    if let (Some(r), Some(c)) = (unknown[r], unknown[c]) {
                        positions.push((r, c));
                    }
                }
            }
            Structure {
                assembly: Assembly::new(n, &positions),
                unknown,
                lu: None,
            }
        });
        let c = self.gas.molar_mass / (R * self.gas.temperature);
        let old: Vec<f64> = self.nodes.iter().map(|n| n.pressure).collect();
        let n = assembly.n();
        let mut converged = false;
        for _ in 0..50 {
            let mut values = Vec::new();
            let mut residual = vec![0.0; n];
            for (i, node) in self.nodes.iter().enumerate() {
                if let (Some(r), GasKind::Store { volume, demand }) = (unknown[i], node.kind) {
                    values.push(c * volume / dt);
                    residual[r] = c * volume * (node.pressure - old[i]) / dt + demand;
                }
            }
            for pipe in &mut self.pipes {
                let (pa, pb) = (self.nodes[pipe.a].pressure, self.nodes[pipe.b].pressure);
                let (m, dpa, dpb) = pipe.flow(&self.gas, pa, pb);
                pipe.flow = m;
                // Leaves a (+ to its residual), enters b (−).
                for (r, col, v) in [
                    (pipe.a, pipe.a, dpa),
                    (pipe.a, pipe.b, dpb),
                    (pipe.b, pipe.a, -dpa),
                    (pipe.b, pipe.b, -dpb),
                ] {
                    if let (Some(_), Some(_)) = (unknown[r], unknown[col]) {
                        values.push(v);
                    }
                }
                if let Some(a) = unknown[pipe.a] {
                    residual[a] += m;
                }
                if let Some(b) = unknown[pipe.b] {
                    residual[b] -= m;
                }
            }
            let matrix = assembly.matrix(&values);
            match lu.as_mut() {
                Some(f) => {
                    f.refactor(&matrix)
                        .expect("a gas network's Jacobian is regular");
                }
                None => lu = Some(Lu::new(&matrix).expect("a gas network's Jacobian is regular")),
            }
            let worst = residual.iter().fold(0.0f64, |m, r| m.max(r.abs()));
            let mut step = residual;
            lu.as_mut().expect("factorised").solve(&mut step);
            let mut largest = 0.0f64;
            for (i, node) in self.nodes.iter_mut().enumerate() {
                if let Some(r) = unknown[i] {
                    // Newton's step, kept from taking a pressure below a tenth of itself.
                    let d = (-step[r]).max(-0.9 * node.pressure);
                    node.pressure += d;
                    largest = largest.max(d.abs() / node.pressure);
                }
            }
            // Converged when the balances held (to a microgram a second) and the step was nil.
            if largest < 1e-12 && worst < 1e-9 {
                converged = true;
                break;
            }
        }
        // The books: what the supplies gave and the demands took.
        let (mut supplied, mut drawn) = (0.0, 0.0);
        for (i, node) in self.nodes.iter().enumerate() {
            match node.kind {
                GasKind::Supply => {
                    let out: f64 = self
                        .pipes
                        .iter()
                        .map(|p| {
                            if p.a == i {
                                p.flow
                            } else if p.b == i {
                                -p.flow
                            } else {
                                0.0
                            }
                        })
                        .sum();
                    if out >= 0.0 {
                        supplied += out * dt
                    } else {
                        drawn -= out * dt
                    }
                }
                GasKind::Store { demand, .. } => drawn += demand * dt,
            }
        }
        self.ledger.import(supplied);
        self.ledger.export(drawn);
        self.structure = Some(Structure {
            unknown,
            assembly,
            lu,
        });
        converged
    }
}
