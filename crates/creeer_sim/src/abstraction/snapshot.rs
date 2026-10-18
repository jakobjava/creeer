//! A machine's abstract self as plain numbers, for saves: its mode, time and stocks, and
//! (abstract) the stretch under way, the last probe's measurement, the rates' scales, with
//! the machine's fast states as it gives them ([`super::Machine::fast`]: a shaft's speed, a
//! controller's memory). Restored, the machine is built anew with them and its stocks
//! written in.

use super::{Abstract, Machine, Mode, Stretch, Trend};

/// A trend as numbers: (0, per second, 0) or (1, per unit of its stock, from).
pub type TrendNumbers = (u8, f64, f64);

/// The stretch under way: its rates, trends, when they were measured, began and end.
#[derive(Clone, Debug, PartialEq)]
pub struct StretchNumbers {
    pub rates: Vec<f64>,
    pub trends: Vec<TrendNumbers>,
    pub at: f64,
    pub began: f64,
    pub ends: f64,
}

/// What a save keeps of a machine's abstract self.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub abstract_mode: bool,
    pub time: f64,
    pub amounts: Vec<f64>,
    pub stretch: Option<StretchNumbers>,
    /// The last probe's (time, rates, stocks).
    pub measured: Option<(f64, Vec<f64>, Vec<f64>)>,
    pub scale: Vec<f64>,
    /// The machine's fast states ([`Machine::fast`]).
    pub fast: Vec<u8>,
}

fn numbers(t: Trend) -> TrendNumbers {
    match t {
        Trend::Time(s) => (0, s, 0.0),
        Trend::Own { beta, at } => (1, beta, at),
    }
}

fn trend((kind, a, b): TrendNumbers) -> Trend {
    if kind == 0 {
        Trend::Time(a)
    } else {
        Trend::Own { beta: a, at: b }
    }
}

impl<M: Machine> Abstract<M> {
    /// Its state as numbers.
    pub fn snapshot(&self) -> Snapshot {
        let abstract_mode = self.mode == Mode::Abstract;
        Snapshot {
            abstract_mode,
            time: self.time,
            amounts: self.amounts(),
            stretch: self
                .stretch
                .as_ref()
                .filter(|_| abstract_mode)
                .map(|s| StretchNumbers {
                    rates: s.rates.clone(),
                    trends: s.trends.iter().map(|&t| numbers(t)).collect(),
                    at: s.at,
                    began: s.began,
                    ends: s.ends,
                }),
            measured: self.measured.clone().filter(|_| abstract_mode),
            scale: self.scale.clone(),
            fast: self.machine.fast(),
        }
    }

    /// `machine` (built anew) as `snapshot` left it: its stocks written in; abstract, its
    /// stretch going on to where it was to end.
    pub fn restore(machine: M, snapshot: &Snapshot) -> Self {
        let mut a = Self::new(machine);
        a.machine.set_fast(&snapshot.fast);
        a.machine.set_amounts(&snapshot.amounts);
        for (&s, &x) in a.ids.iter().zip(&snapshot.amounts) {
            a.network.set_amount(s, x);
        }
        a.time = snapshot.time;
        a.network.set_time(snapshot.time);
        a.scale.clone_from(&snapshot.scale);
        if let (true, Some(s)) = (snapshot.abstract_mode, &snapshot.stretch) {
            a.mode = Mode::Abstract;
            a.measured.clone_from(&snapshot.measured);
            let trends: Vec<Trend> = s.trends.iter().map(|&t| trend(t)).collect();
            let length = (s.ends - snapshot.time).max(0.0);
            a.start(&snapshot.amounts, &s.rates, &trends, s.at, length);
            a.stretch = Some(Stretch {
                rates: s.rates.clone(),
                trends,
                at: s.at,
                began: s.began,
                ends: s.ends,
            });
        }
        a
    }
}
