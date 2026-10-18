//! Machines going abstract and coming back (F7.4, ADR-0020): a machine far from every
//! player runs as rate equations (`creeer_rates`) measured from its own full simulation.
//!
//! - **A machine** names its stocks: what it holds that moves slowly (water in tanks, a
//!   reactor's species and its temperature, heat in large masses, heat given to the air).
//! - **A probe** runs the full machine with the stocks written in: it settles, then two
//!   windows whose rates agree (a transient, a runner spinning down after its tank ran dry,
//!   is waited out); their mean is the machine's rates. The probe's seconds are real time for
//!   the machine: its stocks move on through it.
//! - **Between probes** the rates advance the stocks event to event (`creeer_rates`). The
//!   probes are an integrator's evaluations: Heun's method. A stretch run on the rates `r₀`
//!   found when it began is corrected, once the next probe has found `r₁`, by `½·(r₁ − r₀)·Δ`
//!   (the trapezoidal rule's amount less what was moved): second order, with no derivative
//!   estimated from noisy windows. Each correction conserves what both rates conserve (water,
//!   atoms). The next stretch's length follows how much the rates changed over the last
//!   (an integrator's step control): [`DRIFT`] of change, up to [`LONGEST`].
//! - **A stock at a bound** ends a stretch exactly there (a tank running dry): a new regime,
//!   measured at once; the stretch is not corrected (the rates jumped there, they did not
//!   drift).
//! - **Coming back**, the stocks are written into the full machine, whose fast states are
//!   those of its last probe (settled for nearly these stocks).

mod probe;
pub mod snapshot;

use creeer_rates::{Event, Network, Process, Stock, StockId};

/// The settling window of a probe and each of its two measuring windows (s).
pub const SETTLE: f64 = 5.0;
pub const WINDOW: f64 = 10.0;
/// The change of rates a stretch aims at (relative).
pub const DRIFT: f64 = 0.02;
/// How far two windows' rates may differ for the machine to be steady (a transient, a runner
/// spinning down, changes them far more; a slow ramp, less).
pub const STEADY: f64 = 0.05;
/// The longest a probe waits for the machine to be steady (s).
pub const LONGEST_PROBE: f64 = 600.0;
/// The shortest and longest stretch between probes (s).
pub const SHORTEST: f64 = 60.0;
pub const LONGEST: f64 = 7200.0;

/// One stock of a machine.
#[derive(Clone, Debug, PartialEq)]
pub struct StockDef {
    pub name: String,
    pub low: f64,
    pub high: f64,
    /// Whether it relaxes towards a balance (a mass's heat, a temperature): its rate then
    /// follows its own amount; otherwise time.
    pub relaxes: bool,
}

/// How a stretch's rate changes: per second, or per unit of its own stock from `at`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Trend {
    Time(f64),
    Own { beta: f64, at: f64 },
}

/// A machine that can go abstract.
pub trait Machine {
    /// Its stocks, in a fixed order.
    fn stocks(&self) -> Vec<StockDef>;
    /// Their amounts now.
    fn amounts(&self) -> Vec<f64>;
    /// Writes amounts back (from abstract mode).
    fn set_amounts(&mut self, amounts: &[f64]);
    /// One full tick.
    fn tick(&mut self);
    /// Its tick (s).
    fn dt(&self) -> f64;
    /// Its fast states (a shaft's speed, a controller's memory) as bytes, for a save: a
    /// machine built anew with them goes on running instead of starting from rest.
    fn fast(&self) -> Vec<u8> {
        Vec::new()
    }
    /// Writes back what [`Machine::fast`] gave.
    fn set_fast(&mut self, _bytes: &[u8]) {}
}

/// How a machine is run now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Active,
    Abstract,
}

/// The stretch under way: its rates and when it began.
#[derive(Clone, Debug)]
struct Stretch {
    /// Its rates at `at` (s), and how they change.
    rates: Vec<f64>,
    trends: Vec<Trend>,
    at: f64,
    began: f64,
    /// When it is to end (its clock's trusted box).
    ends: f64,
}

/// A machine with its abstract self.
#[derive(Debug)]
pub struct Abstract<M> {
    pub machine: M,
    mode: Mode,
    /// Its time (s): its own ticks while active, the network's while abstract.
    time: f64,
    network: Network,
    /// The machine's stocks in `network`, then the clock.
    ids: Vec<StockId>,
    clock: StockId,
    stretch: Option<Stretch>,
    /// The last probe's time, rates and stocks (where the next trends start).
    measured: Option<(f64, Vec<f64>, Vec<f64>)>,
    /// The largest rate each stock has had: what its changes are measured against (a mass
    /// come to its balance has a rate of nearly nothing, which any noise changes wholly).
    scale: Vec<f64>,
    /// The stocks at the ends of its last three windows while active, and the ticks since the
    /// last (leaving needs no probe: those rates are measured already).
    recent: Vec<Vec<f64>>,
    since: u64,
    /// Whether the next stretch must begin with a probe (the last ran in full to its end).
    unmeasured: bool,
    /// Probes run, and the full ticks they took.
    pub probes: u32,
    pub probe_ticks: u64,
    /// Events of the abstract stretches.
    pub events: Vec<Event>,
}

impl<M: Machine> Abstract<M> {
    /// `machine`, active, at time 0.
    pub fn new(machine: M) -> Self {
        let mut network = Network::new(0.0);
        let amounts = machine.amounts();
        let ids = (machine.stocks().into_iter().zip(&amounts))
            .map(|(d, &a)| network.add_stock(Stock::new(&d.name, a, d.low, d.high)))
            .collect();
        let clock = network.add_stock(Stock::unbounded("clock"));
        Self {
            machine,
            mode: Mode::Active,
            time: 0.0,
            network,
            ids,
            clock,
            stretch: None,
            measured: None,
            scale: Vec::new(),
            recent: Vec::new(),
            since: 0,
            unmeasured: false,
            probes: 0,
            probe_ticks: 0,
            events: Vec::new(),
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    /// One tick in full (active).
    pub fn tick(&mut self) {
        debug_assert_eq!(self.mode, Mode::Active);
        self.machine.tick();
        self.time += self.machine.dt();
        self.since += 1;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let window = (WINDOW / self.machine.dt()).round() as u64;
        if self.since >= window {
            self.since = 0;
            if self.recent.len() == 3 {
                self.recent.remove(0);
            }
            self.recent.push(self.machine.amounts());
        }
    }

    /// Goes abstract now if it is steady, from the rates of its last two windows run in full
    /// (no probe: the world ran them anyway); returns whether it went. A machine never steady
    /// stays active, as a body never at rest does.
    pub fn try_leave(&mut self) -> bool {
        if self.mode == Mode::Abstract {
            return true;
        }
        let Ok(windows) = <[Vec<f64>; 3]>::try_from(self.recent.clone()) else {
            return false;
        };
        if !probe::steady(&windows) {
            return false;
        }
        let [a0, a1, a2] = windows;
        let rates: Vec<f64> = (a2.iter().zip(&a0))
            .map(|(x, y)| (x - y) / (2.0 * WINDOW))
            .collect();
        #[allow(clippy::cast_precision_loss)]
        let at = self.time - self.since as f64 * self.machine.dt() - WINDOW;
        self.mode = Mode::Abstract;
        let now = self.machine.amounts();
        self.scale = rates.iter().map(|r| r.abs()).collect();
        self.measured = Some((at, rates.clone(), a1));
        let trends = vec![Trend::Time(0.0); rates.len()];
        self.start(&now, &rates, &trends, at, SHORTEST);
        true
    }

    /// The stocks' amounts now.
    pub fn amounts(&self) -> Vec<f64> {
        match self.mode {
            Mode::Active => self.machine.amounts(),
            Mode::Abstract => self
                .ids
                .iter()
                .map(|&s| self.network.stock(s).amount)
                .collect(),
        }
    }

    /// The process the last probe measured, if any.
    pub fn process(&self) -> Option<&Process> {
        self.network.processes().first()
    }

    /// The network's books of stock `k`: (in, out, how far from closing).
    pub fn books(&self, k: usize) -> (f64, f64, f64) {
        let (into, out) = self.network.books(self.ids[k]);
        (into, out, self.network.imbalance(self.ids[k]))
    }

    /// Goes abstract now: measured first (a probe).
    pub fn leave(&mut self) {
        if self.mode == Mode::Abstract {
            return;
        }
        let now = self.machine.amounts();
        for (&s, &a) in self.ids.iter().zip(&now) {
            self.network.set_amount(s, a);
        }
        self.mode = Mode::Abstract;
        self.stretch = None;
        self.probe(false, f64::INFINITY);
    }

    /// Advances abstract time to `until` (s), measured again whenever a stretch ends.
    /// It never goes past `until`: a probe due with less time left than one takes is run in
    /// full to `until` instead (exactly), and the next advance begins with a probe.
    pub fn advance(&mut self, until: f64) {
        while self.time < until {
            if self.unmeasured {
                self.unmeasured = false;
                self.probe(false, until);
                continue;
            }
            let event = self.network.advance_to_event(until);
            self.time = self.network.time();
            if let Some(e) = event {
                self.events.push(e);
                if self.time < until {
                    // A stretch's own end is its clock gone stale; a bound, a new regime.
                    self.probe(matches!(e, Event::Stale { .. }), until);
                }
            }
        }
    }

    /// Comes back: the stocks written into the full machine, active from now.
    pub fn come_back(&mut self) {
        if self.mode == Mode::Active {
            return;
        }
        let amounts = self.amounts();
        self.machine.set_amounts(&amounts);
        self.mode = Mode::Active;
    }
}
