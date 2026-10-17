//! A tracer's step: each part draws off what reaches each pipe's downstream end, settles the
//! nodes upstream first (junctions mix, stores take their inflows in, sources give theirs),
//! then feeds each pipe's upstream end with its node's content.

use super::{Holding, PARTS, Tracer, same};
use creeer_network::Sum;
use std::collections::VecDeque;

/// The step's scratch: each node's inflow (mass, then each quantity's amount), the order the
/// nodes are settled in, and the links holding nothing that leave each node.
#[derive(Clone, Debug, Default)]
pub(super) struct Scratch {
    inflow: Vec<f64>,
    order: Vec<usize>,
    starts: Vec<usize>,
    passing: Vec<usize>,
    waiting: Vec<usize>,
    out: Vec<f64>,
    fill: Vec<usize>,
    /// What came in and went out, each quantity's.
    sums: Vec<Sum>,
}

impl Tracer {
    /// Carries its content by `flows` (kg/s along each link, from its first node to its
    /// second) for `dt` seconds, `draws` (kg/s) taken off at each node (a junction's demand);
    /// the flows a network's step found, which balance at every node that holds nothing.
    /// Returns the parts the step took.
    ///
    /// # Panics
    /// If links that hold nothing (pumps, valves) make a loop the flow goes round: nothing in
    /// it would hold the fluid going round.
    pub fn step(&mut self, flows: &[f64], draws: &[f64], dt: f64) -> usize {
        let n = self.k + 1;
        let flush = (self.lines.iter().zip(flows))
            .filter(|(l, _)| l.holdup > 0.0)
            .map(|(l, m)| m.abs() * dt / l.holdup)
            .fold(0.0f64, f64::max);
        // At most half a pipe's fluid leaves it in a part (so it leaves before any enters).
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let parts = ((2.0 * flush).ceil() as usize).clamp(1, PARTS);
        #[allow(clippy::cast_precision_loss)]
        let tau = dt / parts as f64;
        self.settle_order(flows, draws);
        let mut s = std::mem::take(&mut self.scratch);
        let mut sums = std::mem::take(&mut s.sums);
        sums.clear();
        sums.resize(2 * n, Sum::default());
        let (imported, exported) = sums.split_at_mut(n);
        s.inflow.resize(self.points.len() * n, 0.0);
        for _ in 0..parts {
            s.inflow.fill(0.0);
            for (line, &m) in self.lines.iter_mut().zip(flows) {
                if line.holdup > 0.0 && m != 0.0 {
                    let to = if m > 0.0 { line.b } else { line.a };
                    draw(
                        &mut line.parcels,
                        m.abs() * tau,
                        m > 0.0,
                        &mut s.inflow[to * n..(to + 1) * n],
                    );
                }
            }
            for &i in &s.order {
                let p = &mut self.points[i];
                let inflow = &s.inflow[i * n..(i + 1) * n];
                match p.holding {
                    Holding::Source => {
                        (exported.iter_mut().zip(inflow)).for_each(|(e, x)| e.add(*x))
                    }
                    Holding::Mix => {
                        if inflow[0] > 0.0 {
                            for (c, x) in p.content.iter_mut().zip(&inflow[1..]) {
                                *c = x / inflow[0];
                            }
                        }
                    }
                    Holding::Store => {
                        let mass = p.mass + inflow[0];
                        if mass > 0.0 {
                            for (c, x) in p.content.iter_mut().zip(&inflow[1..]) {
                                *c = (p.mass * *c + x) / mass;
                            }
                        }
                        p.mass = mass - s.out[i] * tau;
                    }
                }
                if p.holding != Holding::Source && draws[i] > 0.0 {
                    let m = draws[i] * tau;
                    exported[0].add(m);
                    (exported[1..].iter_mut().zip(&p.content)).for_each(|(e, c)| e.add(m * c));
                }
                // On through the links that hold nothing, to nodes settled after this one.
                for &l in &s.passing[s.starts[i]..s.starts[i + 1]] {
                    let line = &self.lines[l];
                    let (m, to) = (
                        flows[l].abs() * tau,
                        if flows[l] > 0.0 { line.b } else { line.a },
                    );
                    let into = &mut s.inflow[to * n..(to + 1) * n];
                    into[0] += m;
                    (into[1..].iter_mut().zip(&p.content)).for_each(|(x, c)| *x += m * c);
                }
            }
            for (line, &m) in self.lines.iter_mut().zip(flows) {
                let from = if m > 0.0 { line.a } else { line.b };
                let p = &self.points[from];
                if line.holdup > 0.0 && m != 0.0 {
                    feed(&mut line.parcels, m.abs() * tau, m > 0.0, &p.content);
                }
                if p.holding == Holding::Source && m != 0.0 {
                    let m = m.abs() * tau;
                    imported[0].add(m);
                    (imported[1..].iter_mut().zip(&p.content)).for_each(|(e, c)| e.add(m * c));
                }
            }
        }
        for ((ledger, i), e) in self
            .ledgers
            .iter_mut()
            .zip(imported.iter())
            .zip(exported.iter())
        {
            ledger.import(i.value());
            ledger.export(e.value());
        }
        s.sums = sums;
        self.scratch = s;
        parts
    }

    /// The nodes in an order that settles every link holding nothing's upstream node before
    /// its downstream one (Kahn's), and what leaves each node (kg/s).
    fn settle_order(&mut self, flows: &[f64], draws: &[f64]) {
        let s = &mut self.scratch;
        let count = self.points.len();
        s.out.clear();
        s.out.extend_from_slice(&draws[..count]);
        s.starts.clear();
        s.starts.resize(count + 1, 0);
        s.waiting.clear();
        s.waiting.resize(count, 0);
        let passes = |l: &super::Line, m: f64| l.holdup <= 0.0 && m != 0.0;
        for (l, &m) in self.lines.iter().zip(flows) {
            let (from, to) = if m > 0.0 { (l.a, l.b) } else { (l.b, l.a) };
            s.out[from] += m.abs();
            if passes(l, m) {
                s.starts[from + 1] += 1;
                s.waiting[to] += 1;
            }
        }
        for i in 0..count {
            s.starts[i + 1] += s.starts[i];
        }
        s.passing.clear();
        s.passing.resize(s.starts[count], 0);
        s.fill.clear();
        s.fill.extend_from_slice(&s.starts[..count]);
        for (j, (l, &m)) in self.lines.iter().zip(flows).enumerate() {
            if passes(l, m) {
                let from = if m > 0.0 { l.a } else { l.b };
                s.passing[s.fill[from]] = j;
                s.fill[from] += 1;
            }
        }
        s.order.clear();
        s.order.extend((0..count).filter(|&i| s.waiting[i] == 0));
        let mut next = 0;
        while next < s.order.len() {
            let i = s.order[next];
            next += 1;
            for &l in &s.passing[s.starts[i]..s.starts[i + 1]] {
                let to = if flows[l] > 0.0 {
                    self.lines[l].b
                } else {
                    self.lines[l].a
                };
                s.waiting[to] -= 1;
                if s.waiting[to] == 0 {
                    s.order.push(to);
                }
            }
        }
        assert_eq!(
            s.order.len(),
            count,
            "the flow goes round a loop of links that hold nothing"
        );
    }
}

/// Draws `mass` off a pipe's parcels at its second end (`back`) or its first, adding it and
/// its quantities to `into`.
fn draw(parcels: &mut VecDeque<f64>, mut mass: f64, back: bool, into: &mut [f64]) {
    let n = into.len();
    while mass > 0.0 && !parcels.is_empty() {
        let at = if back { parcels.len() - n } else { 0 };
        let have = parcels[at];
        let take = have.min(mass);
        into[0] += take;
        for q in 1..n {
            into[q] += take * parcels[at + q];
        }
        mass -= take;
        if take >= have {
            if back {
                parcels.truncate(at);
            } else {
                parcels.drain(..n);
            }
        } else {
            parcels[at] = have - take;
        }
    }
}

/// Feeds `mass` of `content` into a pipe at its first end (`front`) or its second, one parcel
/// with its neighbour there if their contents are the same.
fn feed(parcels: &mut VecDeque<f64>, mass: f64, front: bool, content: &[f64]) {
    let n = content.len() + 1;
    if !parcels.is_empty() {
        let at = if front { 0 } else { parcels.len() - n };
        let end = parcels.range(at + 1..at + n);
        if end.zip(content).all(|(a, b)| same(*a, *b)) {
            parcels[at] += mass;
            return;
        }
    }
    if front {
        for &c in content.iter().rev() {
            parcels.push_front(c);
        }
        parcels.push_front(mass);
    } else {
        parcels.push_back(mass);
        parcels.extend(content);
    }
}
