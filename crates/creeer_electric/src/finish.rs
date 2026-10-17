//! After an island's step: each component's current by its own law at the solution, the
//! energy it took in, its new state, and whether it switched (a fuse melting, a breaker
//! tripping, a battery's protection).
//!
//! **Energy.** At the solution Kirchhoff's laws hold, so by Tellegen's theorem the
//! components' v·i sum to zero: what sources give is what the rest take in. Each component
//! books its own: a resistor's as heat; a store's (capacitor, inductor, battery, machine) as
//! the change of what it holds, the rest as heat (its resistances' losses, and where the
//! step cannot resolve a transient, the integration's own damping, booked the same way);
//! a machine's work on its shaft as energy passed on. The ledger then closes to Newton's
//! tolerance (to rounding for a linear circuit).

use crate::circuit::stored_in;
use crate::component::{Component, Kind};
use crate::mna::{self, Place, Rule};

/// A component's energy over a step (J).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Flow {
    /// v·i·h: what it took in (negative: gave).
    pub absorbed: f64,
    /// Given by a source.
    pub supplied: f64,
    /// Turned into heat (or damped by the integration), and how much of that is real heat
    /// the component gives off (to the thermal network).
    pub dissipated: f64,
    pub heat: f64,
    /// Passed on (light, work on a shaft), or taken in from outside (a shaft driving it).
    pub exported: f64,
    pub imported: f64,
}

/// Its current at the solution `x` of a step by `rule`.
fn current(c: &Component, place: Place, x: &[f64], rule: Rule) -> f64 {
    let v = mna::across(x, place.a, place.b);
    let s = &c.state;
    let top = |x: &[f64]| place.a.map_or(0.0, |i| x[i]);
    match &c.kind {
        Kind::Resistor(_)
        | Kind::Wire { .. }
        | Kind::Switch
        | Kind::Fuse { .. }
        | Kind::Breaker { .. } => mna::conductance(c).expect("a conductance") * v,
        Kind::Capacitor(f) => f * (rule.a0 * v / rule.h + rule.history(s.v[0], s.v[1])),
        Kind::Inductor(l) => {
            rule.h / (rule.a0 * l) * v - (rule.a1 * s.i[0] + rule.a2 * s.i[1]) / rule.a0
        }
        Kind::CurrentSource(amps) => *amps,
        Kind::Diode(d) if d.series > 0.0 => (top(x) - x[place.extra]) / d.series,
        Kind::Diode(d) => d.junction(v, s.temperature).0,
        Kind::Solar(m) => (top(x) - x[place.extra]) / m.series,
        Kind::VoltageSource(_) | Kind::Battery(_) | Kind::Machine(_) => x[place.extra],
    }
}

/// The KiBaM wells after `h` seconds of current `i` (charging positive): the bound well's
/// head over the available decays exactly as e^(−λ·h), the total charge changes by i·h.
fn wells(c: &mut Component, i: f64, h: f64) {
    let Kind::Battery(b) = &c.kind else { return };
    let s = &mut c.state;
    let (share, q) = (b.available, b.capacity);
    let total = share * s.level + (1.0 - share) * s.extra[2] + i * h / q;
    let lambda = b.rate * (1.0 / share + 1.0 / (1.0 - share));
    let mu = i / (q * share);
    let head =
        (s.extra[2] - s.level + mu / lambda) * creeer_math::det::exp(-lambda * h) - mu / lambda;
    let available = total - (1.0 - share) * head;
    s.level = available;
    s.extra[2] = available + head;
}

/// Finishes component `c`'s step at solution `x`: its new state and its energy. Returns
/// the flow and whether it switched.
pub(crate) fn finish(c: &mut Component, place: Place, x: &[f64], rule: Rule) -> (Flow, bool) {
    let h = rule.h;
    let v = mna::across(x, place.a, place.b);
    let i = current(c, place, x, rule);
    // During BDF2 the power is averaged over the step (Tellegen holds at both ends, so this
    // closes as exactly, and a resistor's heat is then second order: 1 % off at h/τ = 0.01
    // when taken at the end alone). Right after a restart (backward Euler) at the end only: a
    // jump's energy (a source charging a capacitor at once gives C·V²) is where the step ends.
    #[allow(clippy::float_cmp)] // exactly BDF2's coefficient
    let average = rule.a0 == 1.5;
    let now = v * i;
    let absorbed = if average {
        0.5 * (c.state.v[0] * c.state.i[0] + now) * h
    } else {
        now * h
    };
    let before = stored_in(c);
    let mut flow = Flow {
        absorbed,
        ..Flow::default()
    };
    let mut switched = false;
    {
        let s = &mut c.state;
        s.v = [v, s.v[0]];
        s.i = [i, s.i[0]];
    }
    match &c.kind {
        Kind::Resistor(_) | Kind::Wire { .. } | Kind::Switch => {
            (flow.dissipated, flow.heat) = (absorbed, absorbed)
        }
        Kind::Fuse { melting } => {
            (flow.dissipated, flow.heat) = (absorbed, absorbed);
            let melting = *melting;
            let s = &mut c.state;
            s.level += i * i * h;
            if s.closed && s.level >= melting {
                (s.closed, switched) = (false, true);
            }
        }
        Kind::Breaker {
            rated,
            thermal,
            magnetic,
        } => {
            (flow.dissipated, flow.heat) = (absorbed, absorbed);
            let (rated, thermal, magnetic) = (*rated, *thermal, *magnetic);
            let s = &mut c.state;
            let ratio = i / rated;
            s.level = (s.level + (ratio * ratio - 1.0) * h / thermal).max(0.0);
            if s.closed && (ratio.abs() > magnetic || s.level >= 1.0) {
                (s.closed, switched) = (false, true);
            }
        }
        Kind::Capacitor(_) | Kind::Inductor(_) => {
            flow.dissipated = absorbed - (stored_in(c) - before)
        }
        Kind::VoltageSource(_) | Kind::CurrentSource(_) => flow.supplied = -absorbed,
        Kind::Diode(d) => {
            flow.dissipated = absorbed;
            flow.exported = d.light * absorbed.max(0.0);
            flow.dissipated -= flow.exported;
            flow.heat = flow.dissipated;
        }
        Kind::Solar(_) => {
            if absorbed > 0.0 {
                (flow.dissipated, flow.heat) = (absorbed, absorbed);
            } else {
                flow.supplied = -absorbed;
            }
        }
        Kind::Battery(b) => {
            // The RC pair's voltage, as the step solved it.
            let k = b.c1 * rule.a0 / h + 1.0 / b.r1;
            let s = &c.state;
            let rc = (i - b.c1 * rule.history(s.extra[0], s.extra[1])) / k;
            let (cutoff, open_circuit) = (b.cutoff, b.open_circuit(s.level));
            let s = &mut c.state;
            s.extra[1] = s.extra[0];
            s.extra[0] = rc;
            wells(c, i, h);
            let heat = absorbed - (stored_in(c) - before);
            (flow.dissipated, flow.heat) = (heat, heat);
            let s = &mut c.state;
            if s.closed && i < 0.0 && v < cutoff {
                (s.closed, switched) = (false, true);
            } else if !s.closed && v > open_circuit + 0.05 {
                (s.closed, switched) = (true, true);
            }
        }
        Kind::Machine(m) => {
            let (d, r) = mna::rotor(m, &c.state, rule);
            let s = &mut c.state;
            s.extra[0] = s.level;
            s.level = (r + m.constant * i) / d;
            let work = s.load * s.level * h;
            if work >= 0.0 {
                flow.exported = work;
            } else {
                flow.imported = -work;
            }
            let heat = absorbed - (stored_in(c) - before) - work;
            (flow.dissipated, flow.heat) = (heat, heat);
        }
    }
    (flow, switched)
}

/// Whether `c`'s state still changes in a step that changes nothing else: a fuse carrying
/// current heats, a breaker's bimetal moves while it is warm or overloaded, a battery that
/// carries current charges or drains. Its island must not sleep then.
pub(crate) fn drifting(c: &Component) -> bool {
    let s = &c.state;
    match &c.kind {
        Kind::Fuse { .. } => s.closed && s.i[0] != 0.0,
        Kind::Breaker { rated, .. } => s.closed && (s.level > 0.0 || s.i[0].abs() > *rated),
        Kind::Battery(_) => s.closed && s.i[0] != 0.0,
        _ => false,
    }
}
