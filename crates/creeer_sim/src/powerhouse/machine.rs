//! The powerhouse as a machine that can go abstract (F7.4): its stocks are the water in its
//! upper tank and lower basin, the reactor's hydrogen, oxygen and water, the heat in its three
//! masses, the heat it gave the air, and its reactor's temperature (a slow state: its 500 mol
//! of water against 20 W/K to the coolant, half an hour).

use super::Powerhouse;
use crate::abstraction::{Machine, StockDef};
use creeer_fluids::pipes::{LinkKind, NodeKind};
use creeer_math::codec::{DecodeError, Reader, Writer};

/// The species the reactor's stocks are (moles).
const SPECIES: [&str; 3] = ["H2", "O2", "H2O(L)"];

impl Powerhouse {
    fn tank(&self, node: usize) -> (f64, f64) {
        match self.pipes.nodes[node].kind {
            NodeKind::Tank { area, floor } => (area, floor),
            _ => unreachable!("a tank"),
        }
    }

    /// The heat given to the air so far (J).
    pub fn heat_to_air(&self) -> f64 {
        let (supplied, to_air, ..) = self.heat.ledger().totals();
        to_air - supplied + self.air
    }
}

impl Machine for Powerhouse {
    fn stocks(&self) -> Vec<StockDef> {
        let bounded = |name: &str, low: f64| StockDef {
            name: name.into(),
            low,
            high: f64::INFINITY,
            relaxes: false,
        };
        let free = |name: &str| bounded(name, f64::NEG_INFINITY);
        let relaxing = |name: &str, low: f64| StockDef {
            relaxes: true,
            ..bounded(name, low)
        };
        vec![
            bounded("upper tank (m³)", 0.0),
            bounded("basin (m³)", 0.0),
            bounded("hydrogen (mol)", 0.0),
            bounded("oxygen (mol)", 0.0),
            bounded("reactor water (mol)", 0.0),
            relaxing("generator heat (J)", f64::NEG_INFINITY),
            relaxing("frame heat (J)", f64::NEG_INFINITY),
            relaxing("coolant heat (J)", f64::NEG_INFINITY),
            free("heat to air (J)"),
            relaxing("reactor temperature (K)", 0.0),
        ]
    }

    fn amounts(&self) -> Vec<f64> {
        let volume = |node: usize| {
            let (area, floor) = self.tank(node);
            area * (self.pipes.nodes[node].head - floor)
        };
        let t = self.thermals;
        let mut amounts = vec![volume(self.upper), volume(self.lower)];
        amounts.extend(SPECIES.map(|s| self.sys.index(s).map_or(0.0, |j| self.reactor.moles[j])));
        amounts.extend([t.generator, t.frame, t.coolant].map(|n| self.heat.stored_in(n)));
        amounts.push(self.heat_to_air());
        amounts.push(self.reactor.t);
        amounts
    }

    fn set_amounts(&mut self, amounts: &[f64]) {
        for (node, volume) in [(self.upper, amounts[0]), (self.lower, amounts[1])] {
            let (area, floor) = self.tank(node);
            self.pipes.nodes[node].head = floor + volume / area;
        }
        let mut changed = self.reactor.t.to_bits() != amounts[9].to_bits();
        for (s, &moles) in SPECIES.iter().zip(&amounts[2..5]) {
            if let Some(j) = self.sys.index(s) {
                changed |= self.reactor.moles[j].to_bits() != moles.to_bits();
                self.reactor.moles[j] = moles;
            }
        }
        let t = self.thermals;
        for (n, &joules) in [t.generator, t.frame, t.coolant].iter().zip(&amounts[5..8]) {
            self.heat.set_stored(*n, joules);
        }
        self.air += amounts[8] - self.heat_to_air();
        self.reactor.t = amounts[9];
        if changed {
            // Its integrator starts again from the state written (BDF2's history would take
            // the jump for a rate: hydrogen came seven times too fast for its first steps).
            self.reactor.restart();
        }
    }

    fn tick(&mut self) {
        Powerhouse::tick(self);
    }

    fn dt(&self) -> f64 {
        super::DT
    }

    fn fast(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.f64(self.trains[0].speed(self.runner));
        w.f64(self.trains[0].speed(self.machine[0].node));
        w.f64(self.readings().valve);
        w.bytes_with_length(&self.controls.logic.to_bytes());
        w.finish()
    }

    fn set_fast(&mut self, bytes: &[u8]) {
        let mut r = Reader::new(bytes);
        let read = |r: &mut Reader<'_>| -> Result<(f64, f64, f64, Vec<u8>), DecodeError> {
            Ok((
                r.f64()?,
                r.f64()?,
                r.f64()?,
                r.bytes_with_length()?.to_vec(),
            ))
        };
        let Ok((runner, rotor, valve, logic)) = read(&mut r) else {
            return; // nothing kept: it starts from rest
        };
        let (r0, r1) = (self.runner, self.machine[0].node);
        self.trains[0].set_speed(r0, runner);
        self.trains[0].set_speed(r1, rotor);
        if let LinkKind::Valve { opening, .. } = &mut self.pipes.links[self.valve].kind {
            *opening = valve;
        }
        if let Ok(net) = creeer_logic::Network::from_bytes(&logic) {
            self.controls.logic = net;
        }
    }
}
