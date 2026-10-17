//! A network of stocks and processes, advanced in time segment by segment (`segment.rs`).

use crate::{Process, ProcessId, Stock, StockId};
use creeer_network::Sum;

/// Something that happened while advancing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// A stock reached its lower bound at `time`.
    Empty { stock: StockId, time: f64 },
    /// A stock reached its upper bound at `time`.
    Full { stock: StockId, time: f64 },
    /// A process's stocks left its trusted box at `time`: advancing stopped there, for the
    /// machine to be measured again.
    Stale { process: ProcessId, time: f64 },
}

/// Stocks and the processes between them.
#[derive(Clone, Debug, Default)]
pub struct Network {
    pub(crate) stocks: Vec<Stock>,
    pub(crate) processes: Vec<Process>,
    pub(crate) time: f64,
    /// What came into and went out of each stock since it was added.
    pub(crate) inflow: Vec<Sum>,
    pub(crate) outflow: Vec<Sum>,
    pub(crate) start: Vec<f64>,
    /// Each process's throttle in the last segment.
    pub(crate) throttle: Vec<f64>,
    /// Segments advanced (for budgets and tests).
    pub(crate) segments: u64,
    /// The processes' flows laid out flat: (process, stock), rebuilt when processes change.
    pub(crate) layout: Vec<(usize, usize)>,
    /// The stocks some rate depends on, sorted.
    pub(crate) states: Vec<usize>,
    pub(crate) scratch: Scratch,
    pub(crate) work: crate::exact::Work,
}

/// Buffers a segment reuses.
#[derive(Clone, Debug, Default)]
pub(crate) struct Scratch {
    pub r0: Vec<f64>,
    pub r1: Vec<f64>,
    pub s1: Vec<f64>,
    pub net: Vec<f64>,
    pub free: Vec<f64>,
    pub held: Vec<(usize, crate::throttle::Side)>,
    pub gates: Vec<crate::held::Gate>,
    pub steady: Vec<bool>,
    pub holds: Vec<usize>,
}

impl Network {
    /// An empty network at time `time` (s).
    pub fn new(time: f64) -> Self {
        Self {
            time,
            ..Self::default()
        }
    }

    /// Adds a stock.
    pub fn add_stock(&mut self, stock: Stock) -> StockId {
        self.start.push(stock.amount);
        self.stocks.push(stock);
        self.inflow.push(Sum::default());
        self.outflow.push(Sum::default());
        StockId(self.stocks.len() - 1)
    }

    /// Adds a process.
    pub fn add_process(&mut self, process: Process) -> ProcessId {
        self.processes.push(process);
        self.throttle.push(1.0);
        self.relayout();
        ProcessId(self.processes.len() - 1)
    }

    /// Replaces process `p`'s rates (a machine measured again).
    pub fn set_process(&mut self, p: ProcessId, process: Process) {
        self.processes[p.0] = process;
        self.relayout();
    }

    fn relayout(&mut self) {
        self.layout = (self.processes.iter().enumerate())
            .flat_map(|(p, process)| process.flows.iter().map(move |f| (p, f.stock.0)))
            .collect();
        self.states = self.find_states();
    }

    /// Its time (s).
    pub fn time(&self) -> f64 {
        self.time
    }

    /// Stock `s`.
    pub fn stock(&self, s: StockId) -> &Stock {
        &self.stocks[s.0]
    }

    /// Sets stock `s`'s amount from outside (a machine's full state written back); its books
    /// count it as coming in (or going out).
    pub fn set_amount(&mut self, s: StockId, amount: f64) {
        let change = amount - self.stocks[s.0].amount;
        if change >= 0.0 {
            self.inflow[s.0].add(change);
        } else {
            self.outflow[s.0].add(-change);
        }
        self.stocks[s.0].amount = amount;
    }

    /// Process `p`.
    pub fn process(&self, p: ProcessId) -> &Process {
        &self.processes[p.0]
    }

    /// Process `p`'s throttle in the last segment (1: full rate).
    pub fn throttle(&self, p: ProcessId) -> f64 {
        self.throttle[p.0]
    }

    /// What came into and went out of stock `s` since it was added.
    pub fn books(&self, s: StockId) -> (f64, f64) {
        (self.inflow[s.0].value(), self.outflow[s.0].value())
    }

    /// How far stock `s`'s amount is from its start plus what came in less what went out.
    pub fn imbalance(&self, s: StockId) -> f64 {
        let (into, out) = self.books(s);
        self.stocks[s.0].amount - (self.start[s.0] + into - out)
    }

    /// Segments advanced so far.
    pub fn segments(&self) -> u64 {
        self.segments
    }

    /// Advances to `until` (s), or to where a process goes stale (the last event then);
    /// returns what happened on the way.
    pub fn advance(&mut self, until: f64) -> Vec<Event> {
        let mut events = Vec::new();
        while self.time < until {
            let left = until - self.time;
            let (h, event) = self.segment(left);
            self.segments += 1;
            self.time = if h >= left { until } else { self.time + h };
            if let Some(event) = event {
                let stop = matches!(event, Event::Stale { .. });
                events.push(event);
                if stop {
                    break;
                }
            }
        }
        events
    }
}
