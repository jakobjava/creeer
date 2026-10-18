//! A thermal network: what it is made of, and how it is built and fed.

use crate::convection::Shape;
use crate::enthalpy::Enthalpy;
use crate::fluid::Medium;
use crate::island::IslandSolver;
use creeer_materials::Curve;
use creeer_network::{Islands, Ledger, Sum};
use std::sync::Arc;

/// A node of the network.
pub type NodeId = usize;

/// The Stefan–Boltzmann constant (W/(m²·K⁴)).
pub const STEFAN_BOLTZMANN: f64 = 5.670_374_419e-8;

/// What a node is.
#[derive(Clone, Debug)]
pub enum NodeKind {
    /// A mass of a material: its temperature follows the heat it gains.
    Mass {
        /// Its mass (kg).
        mass: f64,
        /// Its material's enthalpy.
        enthalpy: Arc<Enthalpy>,
    },
    /// A temperature held from outside: the air around, the ground, deep space.
    Fixed,
}

/// How heat passes between two nodes (the first may not be fixed).
#[derive(Clone, Debug)]
pub enum Link {
    /// A fixed conductance (W/K).
    Conductance {
        /// The nodes.
        nodes: (NodeId, NodeId),
        /// W/K.
        conductance: f64,
    },
    /// Conduction through a material: k(T)·A/L, k at the mean temperature.
    Conduction {
        /// The nodes.
        nodes: (NodeId, NodeId),
        /// The material's conductivity.
        conductivity: Arc<Curve>,
        /// Cross-section (m²) and length (m).
        area: f64,
        /// Length (m).
        length: f64,
    },
    /// Convection from a surface (the first node) to the fluid around it (the second), h
    /// from the correlation of its shape at the film temperature.
    Convection {
        /// The surface and the fluid.
        nodes: (NodeId, NodeId),
        /// What the fluid is.
        medium: Medium,
        /// The surface's area (m²).
        area: f64,
        /// Its shape.
        shape: Shape,
        /// The fluid's speed past it (m/s).
        speed: f64,
        /// The fluid's pressure (Pa).
        pressure: f64,
    },
    /// Radiation between two surfaces: σ·F·(T₁⁴ − T₂⁴), F = ε_eff·A·(view factor) (m²).
    Radiation {
        /// The nodes.
        nodes: (NodeId, NodeId),
        /// The exchange factor (m²).
        factor: f64,
    },
}

impl Link {
    /// The two nodes.
    pub fn nodes(&self) -> (NodeId, NodeId) {
        match *self {
            Self::Conductance { nodes, .. }
            | Self::Conduction { nodes, .. }
            | Self::Convection { nodes, .. }
            | Self::Radiation { nodes, .. } => nodes,
        }
    }
}

/// A thermal network.
#[derive(Debug, Default)]
pub struct Network {
    pub(crate) kinds: Vec<NodeKind>,
    pub(crate) temperature: Vec<f64>,
    /// Each mass's specific enthalpy (J/kg): the state the step conserves; its temperature
    /// follows from it.
    pub(crate) enthalpy: Vec<f64>,
    pub(crate) source: Vec<f64>,
    pub(crate) received: Vec<Sum>,
    pub(crate) links: Vec<Link>,
    pub(crate) ledger: Ledger,
    pub(crate) islands: Option<(Islands, Vec<IslandSolver>)>,
    /// Gravity for natural convection (m/s²).
    pub gravity: f64,
}

impl Network {
    /// An empty network under gravity `gravity` (m/s²).
    pub fn new(gravity: f64) -> Self {
        Self {
            gravity,
            ..Self::default()
        }
    }

    /// A mass of `mass` kg of a material with enthalpy `enthalpy`, at `temperature` K.
    pub fn add_mass(&mut self, mass: f64, enthalpy: Arc<Enthalpy>, temperature: f64) -> NodeId {
        assert!(mass > 0.0, "a mass node needs mass");
        let specific = enthalpy.at(temperature);
        self.kinds.push(NodeKind::Mass { mass, enthalpy });
        self.add(temperature, specific, mass * specific)
    }

    /// A node held at `temperature` K.
    pub fn add_fixed(&mut self, temperature: f64) -> NodeId {
        self.kinds.push(NodeKind::Fixed);
        self.add(temperature, 0.0, 0.0)
    }

    fn add(&mut self, temperature: f64, specific: f64, stored: f64) -> NodeId {
        self.temperature.push(temperature);
        self.enthalpy.push(specific);
        self.source.push(0.0);
        self.received.push(Sum::default());
        self.ledger.joined(stored);
        self.islands = None;
        self.temperature.len() - 1
    }

    /// Links two nodes.
    ///
    /// # Panics
    /// If the first node is fixed, or a node does not exist.
    pub fn link(&mut self, link: Link) -> usize {
        let (a, b) = link.nodes();
        assert!(
            a < self.kinds.len() && b < self.kinds.len() && a != b,
            "a link joins two nodes"
        );
        assert!(
            matches!(self.kinds[a], NodeKind::Mass { .. }),
            "a link starts at a mass"
        );
        self.links.push(link);
        self.islands = None;
        self.links.len() - 1
    }

    /// A node's temperature (K).
    pub fn temperature(&self, node: NodeId) -> f64 {
        self.temperature[node]
    }

    /// Sets a fixed node's temperature.
    ///
    /// # Panics
    /// If the node is a mass.
    pub fn set_fixed(&mut self, node: NodeId, temperature: f64) {
        assert!(
            matches!(self.kinds[node], NodeKind::Fixed),
            "only a fixed node is set"
        );
        self.temperature[node] = temperature;
    }

    /// Heat `watts` put into a mass node by a source within the network, from now on.
    pub fn set_source(&mut self, node: NodeId, watts: f64) {
        self.source[node] = watts;
    }

    /// `joules` of heat from another domain (Joule heating, friction, reactions), taken in
    /// at the next step.
    pub fn receive(&mut self, node: NodeId, joules: f64) {
        self.received[node].add(joules);
    }

    /// Sets mass node `node`'s stored heat to `joules` (from 20 °C) from outside (a machine
    /// written back from abstract mode, F7.4); the books count the change as imported (or
    /// exported).
    ///
    /// # Panics
    /// If the node is not a mass.
    pub fn set_stored(&mut self, node: NodeId, joules: f64) {
        let NodeKind::Mass { mass, enthalpy } = &self.kinds[node] else {
            panic!("only a mass stores heat")
        };
        let (mass, h) = (*mass, joules / mass);
        if (mass * self.enthalpy[node]).to_bits() == joules.to_bits() {
            return; // as it is: nothing changes
        }
        let change = mass * (h - self.enthalpy[node]);
        self.temperature[node] = enthalpy.temperature(h, self.temperature[node]);
        self.enthalpy[node] = h;
        if change >= 0.0 {
            self.ledger.import(change);
        } else {
            self.ledger.export(-change);
        }
    }

    /// A mass node's stored heat (J, from 20 °C).
    pub fn stored_in(&self, node: NodeId) -> f64 {
        match &self.kinds[node] {
            NodeKind::Mass { mass, .. } => mass * self.enthalpy[node],
            NodeKind::Fixed => 0.0,
        }
    }

    /// A mass node's specific enthalpy (J/kg, from 20 °C).
    pub fn specific_enthalpy(&self, node: NodeId) -> f64 {
        self.enthalpy[node]
    }

    /// The energy stored (J, enthalpies from 20 °C).
    pub fn stored(&self) -> f64 {
        let mut total = Sum::default();
        for (kind, &h) in self.kinds.iter().zip(&self.enthalpy) {
            if let NodeKind::Mass { mass, .. } = kind {
                total.add(mass * h);
            }
        }
        total.value()
    }

    /// Where the energy went since the network was made.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Takes over the accounts of `earlier`, a network this one replaces holding the same
    /// energy (rebuilt after its parts changed: a member split in two): what was supplied,
    /// dissipated and passed on stays counted.
    pub fn continue_from(&mut self, earlier: &Self) {
        let mut ledger = *earlier.ledger();
        ledger.joined(self.stored() - earlier.stored());
        self.ledger = ledger;
    }

    /// How many islands are awake (being solved).
    pub fn awake(&self) -> usize {
        self.islands
            .as_ref()
            .map_or(0, |(_, s)| s.iter().filter(|i| i.awake()).count())
    }
}
