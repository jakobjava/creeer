//! Networks carrying content: a tracer laid on a liquid or a gas network (its pipes' fluid,
//! its tanks' and stores' masses), carried by each step's flows; and contents made of a
//! chemical mixture (moles of each species and enthalpy per kilogram), whose temperature
//! comes back from the enthalpy.

use super::content::{Holding, Tracer};
use super::gas::{GasKind, GasNetwork};
use super::{LinkKind, Network, NodeKind};
use creeer_chem::System;
use creeer_materials::species::R;

impl Network {
    /// A tracer on it, everything holding `content` (its pipes' water ρ·A·L, its tanks').
    pub fn tracer(&self, content: &[f64]) -> Tracer {
        let rho = self.liquid.density;
        let mut tracer = Tracer::new(content.len());
        for node in &self.nodes {
            let (holding, mass) = match node.kind {
                NodeKind::Reservoir => (Holding::Source, 0.0),
                NodeKind::Junction { .. } => (Holding::Mix, 0.0),
                NodeKind::Tank { area, floor } => {
                    (Holding::Store, rho * area * (node.head - floor))
                }
            };
            tracer.node(holding, mass, content);
        }
        for link in &self.links {
            let holdup = match link.kind {
                LinkKind::Pipe {
                    length, diameter, ..
                } => rho * 0.25 * std::f64::consts::PI * diameter * diameter * length,
                LinkKind::Pump { .. } | LinkKind::Valve { .. } | LinkKind::Turbine { .. } => 0.0,
            };
            tracer.link(link.a, link.b, holdup, content);
        }
        tracer
    }

    /// Carries `tracer`'s content by the flows its last step found, for `dt` seconds; the parts
    /// it took.
    pub fn carry(&self, tracer: &mut Tracer, dt: f64) -> usize {
        let rho = self.liquid.density;
        let (mut flows, mut draws) = std::mem::take(&mut tracer.given);
        flows.clear();
        flows.extend(
            self.links
                .iter()
                .map(|l| if l.open { rho * l.flow } else { 0.0 }),
        );
        draws.clear();
        draws.extend(self.nodes.iter().map(|n| match n.kind {
            NodeKind::Junction { demand } => rho * demand,
            _ => 0.0,
        }));
        let parts = tracer.step(&flows, &draws, dt);
        tracer.given = (flows, draws);
        parts
    }
}

impl GasNetwork {
    /// A tracer on it, everything holding `content` (its stores' line pack; its pipes' gas is
    /// its stores').
    pub fn tracer(&self, content: &[f64]) -> Tracer {
        let c = self.gas.molar_mass / (R * self.gas.temperature);
        let mut tracer = Tracer::new(content.len());
        for node in &self.nodes {
            match node.kind {
                GasKind::Supply => tracer.node(Holding::Source, 0.0, content),
                GasKind::Store { volume, .. } => {
                    tracer.node(Holding::Store, c * volume * node.pressure, content)
                }
            };
        }
        for pipe in &self.pipes {
            tracer.link(pipe.a, pipe.b, 0.0, content);
        }
        tracer
    }

    /// Carries `tracer`'s content by the flows its last step found, for `dt` seconds.
    pub fn carry(&self, tracer: &mut Tracer, dt: f64) -> usize {
        let flows: Vec<f64> = self.pipes.iter().map(|p| p.flow).collect();
        let draws: Vec<f64> = (self.nodes.iter())
            .map(|n| {
                if let GasKind::Store { demand, .. } = n.kind {
                    demand
                } else {
                    0.0
                }
            })
            .collect();
        tracer.step(&flows, &draws, dt)
    }
}

/// A content of a mixture of `sys`'s species (`moles`, any amount) at `t`: moles of each per
/// kilogram, then its enthalpy per kilogram (J/kg, from the elements at 298.15 K).
pub fn of_mixture(sys: &System, moles: &[f64], t: f64) -> Vec<f64> {
    let mass = sys.mass(moles);
    let mut c: Vec<f64> = moles.iter().map(|n| n / mass).collect();
    c.push(sys.enthalpy(moles, t) / mass);
    c
}

/// The temperature (K) of a mixture's `content` (as [`of_mixture`] makes it), by Newton's method
/// on its enthalpy from `guess`.
pub fn temperature(sys: &System, content: &[f64], guess: f64) -> f64 {
    let (moles, h) = content.split_at(content.len() - 1);
    let mut t = guess;
    for _ in 0..50 {
        let step = (sys.enthalpy(moles, t) - h[0]) / sys.heat_capacity(moles, t);
        t -= step;
        if step.abs() < 1e-10 * t {
            break;
        }
    }
    t
}
