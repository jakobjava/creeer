//! The lab's running experiments: a reaction relaxing to its equilibrium, and a furnace
//! reducing iron ore with carbon monoxide.

use creeer_chem::kinetics::Reaction;
use creeer_chem::reactor::{Holding, Reactor};
use creeer_chem::{Fixed, System, equilibrate};
use creeer_materials::species::SpeciesTable;
use std::collections::VecDeque;

/// Seconds of trace kept.
const WINDOW: f64 = 30.0;

/// N₂O₄ ⇌ 2 NO₂ at 1 atm, held at a temperature you set: the brown gas dissociating as it
/// warms, recombining as it cools. Its rate is slowed so the eye can follow it (the real
/// reaction settles in microseconds); where it settles is the species' data.
pub struct Relaxation {
    pub sys: System,
    reactor: Reactor,
    pub t: f64,
    pub trace: VecDeque<[f64; 2]>,
    pub equilibrium: f64,
    time: f64,
}

impl Relaxation {
    pub fn new(table: &SpeciesTable) -> Self {
        let sys = System::new(table, &["N2O4", "NO2"]).expect("nitrogen oxides");
        let reaction = Reaction::new(&sys, &[("N2O4", 1.0)], &[("NO2", 2.0)], 0.5, 0.0, 0.0)
            .expect("balanced");
        let mut moles = vec![0.0; 2];
        moles[sys.index("N2O4").expect("N2O4")] = 1.0;
        let mut reactor = Reactor::new(
            &sys,
            moles,
            300.0,
            Holding::Pressure(101_325.0),
            vec![reaction],
        );
        reactor.isothermal = true;
        Self {
            sys,
            reactor,
            t: 300.0,
            trace: VecDeque::new(),
            equilibrium: 0.0,
            time: 0.0,
        }
    }

    /// The share of the gas that is NO₂.
    fn fraction(&self, moles: &[f64]) -> f64 {
        moles[self.sys.index("NO2").expect("NO2")] / moles.iter().sum::<f64>()
    }

    pub fn tick(&mut self, dt: f64) {
        if (self.reactor.t - self.t).abs() > 1e-9 {
            self.reactor.t = self.t;
            self.reactor.restart();
        }
        self.reactor.step(&self.sys, dt);
        self.time += dt;
        let eq = equilibrate(
            &self.sys,
            &self.reactor.moles,
            Fixed::Tp {
                t: self.t,
                p: 101_325.0,
            },
            None,
        );
        self.equilibrium = eq.map_or(f64::NAN, |e| self.fraction(&e.moles));
        let x = self.fraction(&self.reactor.moles);
        self.trace.push_back([self.time, x]);
        while self
            .trace
            .front()
            .is_some_and(|p| p[0] < self.time - WINDOW)
        {
            self.trace.pop_front();
        }
    }
}

/// The furnace's species.
const FURNACE: [&str; 9] = [
    "CO",
    "CO2",
    "O2",
    "N2",
    "Fe(a)",
    "Fe(c)",
    "Fe.947O(cr)",
    "Fe3O4(cr)",
    "Fe2O3(cr)",
];

/// A bed of hematite at a temperature you set, carbon monoxide blown through it: each tick a
/// parcel of gas comes to equilibrium with the bed and leaves (fast reactions: the gas leaves
/// in equilibrium with the solids it last touched). The oxides walk down, hematite to
/// magnetite to wüstite to iron, the gas using less of its CO at each.
pub struct Furnace {
    pub sys: System,
    bed: Vec<f64>,
    pub t: f64,
    /// The solids (Fe₂O₃, Fe₃O₄, FeO, Fe; mol) and the off-gas's CO share, against time.
    pub solids: [VecDeque<[f64; 2]>; 4],
    pub off_gas: VecDeque<[f64; 2]>,
    time: f64,
}

impl Furnace {
    pub fn new(table: &SpeciesTable) -> Self {
        let sys = System::new(table, &FURNACE).expect("the furnace's species");
        let mut bed = vec![0.0; sys.species.len()];
        bed[sys.index("Fe2O3(cr)").expect("hematite")] = 2.0;
        Self {
            sys,
            bed,
            t: 1100.0,
            solids: Default::default(),
            off_gas: VecDeque::new(),
            time: 0.0,
        }
    }

    pub fn tick(&mut self, dt: f64) {
        let mut feed = self.bed.clone();
        let at = |name: &str| self.sys.index(name).expect("a furnace species");
        // A parcel: 0.05 mol of CO with 0.05 of N₂ (5 L/s of gas at room conditions).
        feed[at("CO")] += 0.05 * dt / 0.1;
        feed[at("N2")] += 0.05 * dt / 0.1;
        let Ok(eq) = equilibrate(
            &self.sys,
            &feed,
            Fixed::Tp {
                t: self.t,
                p: 101_325.0,
            },
            None,
        ) else {
            return;
        };
        let gas: f64 = eq.moles[..self.sys.gases].iter().sum();
        self.bed = eq.moles.clone();
        for j in 0..self.sys.gases {
            self.bed[j] = 0.0;
        }
        self.time += dt;
        let iron = eq.moles[at("Fe(a)")] + eq.moles[at("Fe(c)")];
        let amounts = [
            eq.moles[at("Fe2O3(cr)")],
            eq.moles[at("Fe3O4(cr)")],
            eq.moles[at("Fe.947O(cr)")],
            iron,
        ];
        for (trace, n) in self.solids.iter_mut().zip(amounts) {
            trace.push_back([self.time, n]);
        }
        self.off_gas
            .push_back([self.time, eq.moles[at("CO")] / gas * 100.0]);
        // Half an hour of history at most.
        for trace in self
            .solids
            .iter_mut()
            .chain(std::iter::once(&mut self.off_gas))
        {
            while trace.len() > 18_000 {
                trace.pop_front();
            }
        }
    }
}
