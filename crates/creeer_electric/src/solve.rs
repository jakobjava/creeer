//! An island's step: Newton's method on its modified nodal equations, discretised by
//! BDF2 (backward Euler for two steps after a restart: the first step, a switch, a fuse;
//! BDF2's extrapolation from a history spanning the jump overshoots it by about h/τ of
//! the jump, from two steps after by about (h/τ)² of it).
//!
//! **Why BDF2, not the trapezoidal rule.** A 60 Hz tick is far longer than many circuits'
//! own time constants (a microhenry of wire, a capacitor behind an ohm). The trapezoidal
//! rule's amplification tends to −1 for those: they would ring at the tick's rate forever,
//! sign flipping each step. BDF2 is second order and L-stable (fast modes die in a step),
//! backward Euler first order and L-stable; both settle what the tick cannot resolve to
//! where it settles. What the tick does resolve keeps BDF2's accuracy.
//!
//! **Charge.** BDF2's current, C·(3/2·v₊ − 2·v₀ + 1/2·v₋)/h, does not sum over steps to
//! C·Δv as backward Euler's does: the charge delivered differs from what the capacitor holds
//! by boundary terms, ½·C·(v_N − v_N₋₁) − ½·C·(v₂ − v₁) from the restart on. Bounded (it never
//! accumulates) and zero once settled; the energy it implies is booked where it arises, so
//! the ledger still closes.

use crate::component::{Component, Kind};
use crate::finish::Flow;
use crate::mna::{self, GMIN, Pattern, Place, Rule, Values};
use crate::nonlinear::{self, Lin};
use creeer_math::sparse::{Assembly, Csc, Lu};

/// Newton stops when no unknown changes by more than this plus [`RELATIVE`] of itself
/// (V, A, rad/s).
const ABSOLUTE: f64 = 1e-9;
const RELATIVE: f64 = 1e-9;
/// ... and a device's current is within this (A) plus [`RELATIVE`] of itself of its
/// linearisation.
const CURRENT: f64 = 1e-12;
/// Iterations at most.
const MAX_ITERATIONS: usize = 60;
/// Steps an island's unknowns must hardly change for (by [`STILL`]) before it sleeps.
const STILL_STEPS: u32 = 30;
const STILL: f64 = 1e-7;

/// An island of a circuit and its solver.
#[derive(Debug)]
pub(crate) struct IslandSolver {
    /// Its components (global ids) and where their unknowns are.
    pub components: Vec<usize>,
    pub places: Vec<Place>,
    /// Its nodes (global), local index = position.
    pub nodes: Vec<usize>,
    assembly: Assembly,
    matrix: Csc<f64>,
    lu: Option<Lu<f64>>,
    factored: Vec<f64>,
    values: Vec<f64>,
    rhs: Vec<f64>,
    /// The unknowns.
    pub x: Vec<f64>,
    nonlinear: bool,
    /// Each component's first matrix entry in the pattern, whether its entries change with
    /// Newton's iterate (diodes, solar modules), and the rule the others were last stamped
    /// with (none: stamp them all again).
    offsets: Vec<usize>,
    dynamic: Vec<bool>,
    stamped: Option<Rule>,
    /// The nonlinear devices (local indices) and their linearisations; the components with
    /// a right-hand side.
    dynamic_list: Vec<usize>,
    lins: Vec<Lin>,
    with_rhs: Vec<usize>,
    /// Steps since it last restarted, and the step those were made with.
    steps: u32,
    h: f64,
    still: u32,
    /// Asleep: steps reuse the last step's flows.
    pub asleep: bool,
    /// How far its last step moved any unknown.
    pub moved: f64,
    /// The rule of its last step.
    pub rule: Rule,
    /// The last step's flows per component (absorbed energy, J), for sleeping steps.
    pub flows: Vec<Flow>,
    /// The last step's leakage through GMIN (J).
    pub leak: f64,
    /// The power leaking through GMIN at its end (W).
    pub leak_power: f64,
}

/// What an island's step did.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Outcome {
    pub iterations: usize,
    pub factorisations: usize,
    pub converged: bool,
}

impl IslandSolver {
    /// The solver of the island of `components` on `nodes` (global node → local index in
    /// `local`).
    pub fn new(
        all: &[Component],
        components: Vec<usize>,
        nodes: Vec<usize>,
        local: &[Option<usize>],
    ) -> Self {
        let mut size = nodes.len();
        let mut places = Vec::with_capacity(components.len());
        for &k in &components {
            let (a, b) = mna::terminals(&all[k], local);
            places.push(Place { a, b, extra: size });
            size += mna::extras(&all[k]);
        }
        let x = vec![0.0; size];
        let mut pattern = Pattern((0..nodes.len()).map(|i| (i, i)).collect());
        let mut offsets = Vec::with_capacity(components.len());
        for (&k, &place) in components.iter().zip(&places) {
            offsets.push(pattern.0.len());
            mna::stamp(&mut pattern, &all[k], place, &x, Rule::euler(1.0));
        }
        let assembly = Assembly::new(size, &pattern.0);
        Self {
            nonlinear: components.iter().any(|&k| mna::nonlinear(&all[k])),
            dynamic: components
                .iter()
                .map(|&k| mna::nonlinear(&all[k]))
                .collect(),
            dynamic_list: (0..components.len())
                .filter(|&j| mna::nonlinear(&all[components[j]]))
                .collect(),
            lins: vec![
                Lin::default();
                components
                    .iter()
                    .filter(|&&k| mna::nonlinear(&all[k]))
                    .count()
            ],
            with_rhs: (0..components.len())
                .filter(|&j| mna::has_rhs(&all[components[j]]))
                .collect(),
            offsets,
            stamped: None,
            matrix: assembly.zeros(),
            assembly,
            lu: None,
            factored: Vec::new(),
            values: Vec::new(),
            rhs: vec![0.0; size],
            flows: vec![Flow::default(); components.len()],
            leak: 0.0,
            leak_power: 0.0,
            components,
            places,
            nodes,

            x,
            steps: 0,
            h: 0.0,
            still: 0,
            asleep: false,
            rule: Rule::euler(1.0),
            moved: 0.0,
        }
    }

    /// Whether component `id` is in it.
    pub fn has(&self, id: usize) -> bool {
        self.components.binary_search(&id).is_ok()
    }

    /// Something acting on it changed.
    pub fn wake(&mut self) {
        self.stamped = None;
        self.asleep = false;
        self.still = 0;
    }

    /// A right-hand side acting on it changed: step again, its matrix as it was.
    pub fn stir(&mut self) {
        self.asleep = false;
        self.still = 0;
    }

    /// A component switched: wake, and integrate from here by backward Euler.
    pub fn restart(&mut self) {
        self.wake();
        self.steps = 0;
    }

    /// Solves the step of `h` seconds: the unknowns `x` at its end (the components' states
    /// are its history, unchanged here).
    pub fn solve(&mut self, all: &[Component], h: f64) -> Outcome {
        #[allow(clippy::float_cmp)] // the very same step, not a close one
        let rule = if self.steps >= 2 && self.h == h {
            Rule::bdf2(h)
        } else {
            Rule::euler(h)
        };
        self.rule = rule;
        let mut outcome = Outcome::default();
        let start = self.x.clone();
        let mut y = self.x.clone();
        for _ in 0..MAX_ITERATIONS {
            outcome.iterations += 1;
            let changed = self.stamp(all, rule);
            if self.lu.is_none() || (changed && self.values != self.factored) {
                self.assembly.fill(&self.values, &mut self.matrix.values);
                let singular = "a circuit's matrix is regular (GMIN ties every node to ground)";
                match &mut self.lu {
                    Some(lu) => {
                        lu.refactor(&self.matrix).expect(singular);
                    }
                    None => self.lu = Some(Lu::new(&self.matrix).expect(singular)),
                }
                self.factored.clone_from(&self.values);
                outcome.factorisations += 1;
            }
            y.clone_from(&self.rhs);
            self.lu.as_mut().expect("factorised").solve(&mut y);
            if !self.nonlinear {
                std::mem::swap(&mut self.x, &mut y);
                outcome.converged = true;
                break;
            }
            let limited = self.limit(all, &mut y);
            // Converged when the devices' linearisation held at the new iterate (their exact
            // currents there within tolerance of the linear ones the solve used: the
            // circuit's own equations hold), or the iterates stopped moving.
            let exact = !limited && self.exact(all, &y);
            let still = exact
                || y.iter()
                    .zip(&self.x)
                    .all(|(n, o)| (n - o).abs() <= ABSOLUTE + RELATIVE * n.abs().max(o.abs()));
            std::mem::swap(&mut self.x, &mut y);
            if still {
                outcome.converged = true;
                break;
            }
        }
        self.moved = self
            .x
            .iter()
            .zip(&start)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        self.steps += 1;
        self.h = h;
        outcome
    }

    /// Stamps the island at its iterate `x`: every entry when the rule or anything about the
    /// island changed, else the nonlinear devices' entries in place and the right-hand sides
    /// of the components that have one. Returns whether any matrix entry may have changed.
    fn stamp(&mut self, all: &[Component], rule: Rule) -> bool {
        self.rhs.fill(0.0);
        if self.stamped == Some(rule) {
            let mut changed = false;
            for &j in &self.with_rhs {
                mna::stamp(
                    &mut mna::RhsOnly(&mut self.rhs),
                    &all[self.components[j]],
                    self.places[j],
                    &self.x,
                    rule,
                );
            }
            self.linearise(all);
            for (d, &j) in self.dynamic_list.iter().enumerate() {
                let (c, place, lin) = (&all[self.components[j]], self.places[j], self.lins[d]);
                let mut sink = mna::At {
                    values: &mut self.values,
                    at: self.offsets[j],
                    rhs: &mut self.rhs,
                    changed: false,
                };
                nonlinear::stamp(&mut sink, c, place, lin);
                changed |= sink.changed;
            }
            return changed;
        }
        self.linearise(all);
        self.values.clear();
        self.values
            .extend(std::iter::repeat_n(GMIN, self.nodes.len()));
        let mut sink = Values {
            values: &mut self.values,
            rhs: &mut self.rhs,
        };
        let mut d = 0;
        for (j, (&k, &place)) in self.components.iter().zip(&self.places).enumerate() {
            if self.dynamic[j] {
                nonlinear::stamp(&mut sink, &all[k], place, self.lins[d]);
                d += 1;
            } else {
                mna::stamp(&mut sink, &all[k], place, &self.x, rule);
            }
        }
        self.stamped = Some(rule);
        true
    }

    /// Newton's junction voltages limited (`Diode::limit`): each nonlinear device's internal
    /// node moved so its junction steps no further than the limit allows. Whether any was.
    fn limit(&self, all: &[Component], y: &mut [f64]) -> bool {
        let mut limited = false;
        for &j in &self.dynamic_list {
            let (c, place) = (&all[self.components[j]], self.places[j]);
            if !matches!(c.kind, Kind::Diode(d) if d.series > 0.0)
                && !matches!(c.kind, Kind::Solar(_) | Kind::Electrolyzer(_))
            {
                continue;
            }
            let below = |x: &[f64]| place.b.map_or(0.0, |i| x[i]);
            let (new, old) = (
                y[place.extra] - below(y),
                self.x[place.extra] - below(&self.x),
            );
            let t = c.state.temperature;
            let held = match &c.kind {
                Kind::Diode(d) => d.limit(new, old, t),
                Kind::Solar(m) => {
                    let (_, i0) = m.sources(c.state.extra[0], t);
                    let diode = crate::devices::Diode {
                        saturation: i0,
                        emission: m.ideality * m.cells,
                        series: 0.0,
                        light: 0.0,
                    };
                    diode.limit(new, old, t)
                }
                Kind::Electrolyzer(cell) => cell.limit(new, old, c.state.extra[0], t),
                _ => continue,
            };
            #[allow(clippy::float_cmp)] // whether it was moved at all
            let moved = held != new;
            limited |= moved;
            y[place.extra] = below(y) + held;
        }
        limited
    }

    /// Each nonlinear device linearised at the iterate. (In parallel it was slower: the
    /// work is a few thousand exponentials, less than waking the threads.)
    fn linearise(&mut self, all: &[Component]) {
        for (&j, lin) in self.dynamic_list.iter().zip(self.lins.iter_mut()) {
            *lin = nonlinear::linearise(&all[self.components[j]], self.places[j], &self.x);
        }
    }

    /// Whether every device's linearisation held at `y` (see `solve`).
    fn exact(&self, all: &[Component], y: &[f64]) -> bool {
        self.dynamic_list.iter().zip(&self.lins).all(|(&j, &lin)| {
            let (e, i) = nonlinear::error(&all[self.components[j]], self.places[j], lin, y);
            e <= CURRENT + RELATIVE * i
        })
    }

    /// After a step that moved its unknowns by at most `moved`: whether it may now sleep.
    pub fn settle(&mut self, moved: f64) {
        self.still = if moved <= STILL { self.still + 1 } else { 0 };
        self.asleep = self.still >= STILL_STEPS;
    }
}
