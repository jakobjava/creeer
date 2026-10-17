//! Pipe networks of a liquid (F6.6): junctions, reservoirs and tanks joined by pipes, pumps
//! and valves, solved by the global gradient method (Todini and Pilati, as EPANET): Newton's
//! method on the heads and flows together, the flows eliminated so the heads come from one
//! symmetric positive definite system (the sparse Cholesky). Tanks store by backward Euler.
//!
//! Heads are hydraulic heads H = z + p/(ρg) (m); flows are volumetric (m³/s), from a link's
//! first node to its second. Every joule is booked: pumps' work in, friction out as heat,
//! water's energy ρ·g·H·Q in and out of reservoirs and consumers, the tanks' potential energy
//! held.

mod carry;
pub mod content;
pub mod friction;
pub mod gas;

pub use carry::{of_mixture, temperature};
#[cfg(test)]
mod carry_tests;
#[cfg(test)]
mod gas_tests;
mod links;
mod liquid;
pub mod schedule;
#[cfg(test)]
mod schedule_tests;
mod solve;
#[cfg(test)]
mod tests;

use creeer_network::Ledger;

/// A liquid's density (kg/m³) and dynamic viscosity (Pa·s).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Liquid {
    pub density: f64,
    pub viscosity: f64,
}

impl Liquid {
    /// Water at 20 °C (IAPWS: 998.21 kg/m³, 1.0016 mPa·s).
    pub const WATER_20C: Self = Self {
        density: 998.21,
        viscosity: 1.0016e-3,
    };
}

/// What a node is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NodeKind {
    /// A junction: what flows in flows out, less `demand` (m³/s) drawn off there.
    Junction { demand: f64 },
    /// A reservoir: its head held (a lake, the sea, the mains).
    Reservoir,
    /// A tank of plan area `area` (m²), its floor at `floor` (m): its head is its water's
    /// level, rising with what flows in.
    Tank { area: f64, floor: f64 },
}

/// A node: its elevation (m), its kind, its head (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub elevation: f64,
    pub kind: NodeKind,
    pub head: f64,
}

/// What a link is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LinkKind {
    /// A pipe: length (m), inner diameter (m), wall roughness (m), its fittings' minor loss
    /// coefficients summed, and whether a check valve lets it flow forward only.
    Pipe {
        length: f64,
        diameter: f64,
        roughness: f64,
        minor: f64,
        check: bool,
    },
    /// A centrifugal pump (forward only): head H = s²·H₀ − r·Q|Q| at relative speed s
    /// (affinity laws), its efficiency η_best at flow Q_best·s, falling parabolically.
    Pump {
        shutoff: f64,
        resistance: f64,
        speed: f64,
        best_flow: f64,
        best_efficiency: f64,
    },
    /// A valve of diameter `diameter` (m): loss coefficient `open` when open, open/φ² at
    /// opening φ (0 closed).
    Valve {
        diameter: f64,
        open: f64,
        opening: f64,
    },
}

/// A link from node `a` to node `b`, its flow (m³/s), and whether it is open (a check valve
/// or a pump may close against the flow, a valve be shut).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    pub a: usize,
    pub b: usize,
    pub kind: LinkKind,
    pub flow: f64,
    pub open: bool,
}

/// A step's outcome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PipeReport {
    pub iterations: usize,
    pub converged: bool,
}

/// A liquid's pipe network.
#[derive(Clone, Debug)]
pub struct Network {
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub liquid: Liquid,
    pub gravity: f64,
    pub(crate) ledger: Ledger,
    pub(crate) solver: Option<liquid::Solver>,
    /// Ticks gone by (`schedule.rs`: its rate falls on some).
    pub(crate) ticks: u64,
}

impl Network {
    /// An empty network of `liquid` under gravity `gravity` (m/s²).
    pub fn new(liquid: Liquid, gravity: f64) -> Self {
        Self {
            nodes: Vec::new(),
            links: Vec::new(),
            liquid,
            gravity,
            ledger: Ledger::new(0.0),
            solver: None,
            ticks: 0,
        }
    }

    /// Adds a node at `elevation` with head `head` (a reservoir's level, a tank's water).
    pub fn node(&mut self, elevation: f64, kind: NodeKind, head: f64) -> usize {
        let held = self.stored();
        self.nodes.push(Node {
            elevation,
            kind,
            head,
        });
        self.ledger.joined(self.stored() - held);
        self.solver = None;
        self.nodes.len() - 1
    }

    /// Adds a link from `a` to `b`.
    pub fn link(&mut self, a: usize, b: usize, kind: LinkKind) -> usize {
        self.links.push(Link {
            a,
            b,
            kind,
            flow: 0.0,
            open: true,
        });
        self.solver = None;
        self.links.len() - 1
    }

    /// A node's gauge pressure (Pa): ρ·g·(H − z).
    pub fn pressure(&self, node: usize) -> f64 {
        let n = &self.nodes[node];
        self.liquid.density * self.gravity * (n.head - n.elevation)
    }

    /// The energy the network holds (J): its tanks' water's potential energy above its floor
    /// level, ρ·g·A·(H² − floor²)/2.
    pub fn stored(&self) -> f64 {
        let rg = self.liquid.density * self.gravity;
        self.nodes
            .iter()
            .map(|n| match n.kind {
                NodeKind::Tank { area, floor } => {
                    0.5 * rg * area * (n.head * n.head - floor * floor)
                }
                _ => 0.0,
            })
            .sum()
    }

    /// Where its energy went.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }
}
