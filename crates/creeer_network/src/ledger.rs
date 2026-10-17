//! Energy ledgers: every joule a network takes in, gives off or keeps, summed without
//! rounding drift (compensated sums: a long run adds millions of small amounts).

/// A compensated sum (Neumaier's variant of Kahan summation): its error does not grow with the
/// number of terms or their order of sizes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sum {
    sum: f64,
    compensation: f64,
}

impl Sum {
    /// Adds `x`.
    pub fn add(&mut self, x: f64) {
        let t = self.sum + x;
        if self.sum.abs() >= x.abs() {
            self.compensation += (self.sum - t) + x;
        } else {
            self.compensation += (x - t) + self.sum;
        }
        self.sum = t;
    }

    /// The total.
    pub fn value(&self) -> f64 {
        self.sum + self.compensation
    }
}

/// Where a network's energy went since it started being counted.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ledger {
    /// Stored when counting began (J).
    initial: f64,
    /// Put in by sources within the network: batteries, fuel, sunlight (J).
    supplied: Sum,
    /// Turned into heat within it, or lost to the surroundings (J).
    dissipated: Sum,
    /// Passed to other domains (J).
    exported: Sum,
    /// Received from other domains (J).
    imported: Sum,
}

impl Ledger {
    /// Counting from now, with `stored` joules held.
    pub fn new(stored: f64) -> Self {
        Self {
            initial: stored,
            ..Self::default()
        }
    }

    /// Something already holding `joules` joined the network (a part added): it adds to the
    /// store counted from, not to what was supplied.
    pub fn joined(&mut self, joules: f64) {
        self.initial += joules;
    }

    /// A source put in `joules`.
    pub fn supply(&mut self, joules: f64) {
        self.supplied.add(joules);
    }

    /// `joules` were dissipated.
    pub fn dissipate(&mut self, joules: f64) {
        self.dissipated.add(joules);
    }

    /// `joules` went to another domain.
    pub fn export(&mut self, joules: f64) {
        self.exported.add(joules);
    }

    /// `joules` came from another domain.
    pub fn import(&mut self, joules: f64) {
        self.imported.add(joules);
    }

    /// The totals: (supplied, dissipated, exported, imported).
    pub fn totals(&self) -> (f64, f64, f64, f64) {
        (
            self.supplied.value(),
            self.dissipated.value(),
            self.exported.value(),
            self.imported.value(),
        )
    }

    /// The joules unaccounted for, with `stored` held now: zero for a network that conserves
    /// energy.
    pub fn imbalance(&self, stored: f64) -> f64 {
        let (s, d, e, i) = self.totals();
        let mut balance = Sum::default();
        for x in [stored, -self.initial, -s, d, e, -i] {
            balance.add(x);
        }
        balance.value()
    }

    /// The imbalance relative to the largest flow or store: what "to rounding" is measured
    /// against.
    pub fn relative_imbalance(&self, stored: f64) -> f64 {
        let (s, d, e, i) = self.totals();
        let scale = [stored, self.initial, s, d, e, i]
            .iter()
            .fold(f64::MIN_POSITIVE, |m, x| m.max(x.abs()));
        self.imbalance(stored).abs() / scale
    }
}
