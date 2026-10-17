//! The nonlinear devices' part of Newton's method: each linearised once per iteration (its
//! junction's voltage, current and slope), stamped from that, and checked against its exact
//! law at the new iterate (which, where it holds, makes the iterate the solution).

use crate::component::{Component, Kind};
use crate::mna::{Place, Sink};

/// A device's junction linearised at Newton's iterate: voltage (V), current (A), slope (S).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lin {
    pub v: f64,
    pub i: f64,
    pub g: f64,
}

/// The node below a device's junction (its second terminal), and the junction's top: its
/// internal node behind the series resistance, or its first terminal.
fn junction_nodes(c: &Component, place: Place) -> (Option<usize>, Option<usize>) {
    match &c.kind {
        Kind::Diode(d) if d.series <= 0.0 => (place.a, place.b),
        _ => (Some(place.extra), place.b),
    }
}

/// The junction's voltage at `x`.
pub fn junction_voltage(c: &Component, place: Place, x: &[f64]) -> f64 {
    let (top, below) = junction_nodes(c, place);
    top.map_or(0.0, |i| x[i]) - below.map_or(0.0, |i| x[i])
}

/// The junction's current and slope at voltage `v`, if `c` is a nonlinear device.
pub fn law(c: &Component, v: f64) -> Option<(f64, f64)> {
    let t = c.state.temperature;
    match &c.kind {
        Kind::Diode(d) => Some(d.junction(v, t)),
        Kind::Solar(m) => {
            let (_, i0) = m.sources(c.state.extra[0], t);
            let e = creeer_math::det::exp(v / m.thermal_voltage(t));
            Some((i0 * (e - 1.0), i0 * e / m.thermal_voltage(t)))
        }
        Kind::Electrolyzer(cell) => Some(cell.junction(v, c.state.extra[0], t)),
        _ => None,
    }
}

/// Linearises device `c` at iterate `x`.
pub fn linearise(c: &Component, place: Place, x: &[f64]) -> Lin {
    let v = junction_voltage(c, place, x);
    let (i, g) = law(c, v).expect("a nonlinear device");
    Lin { v, i, g }
}

/// Stamps device `c` linearised as `lin`.
pub fn stamp<S: Sink>(s: &mut S, c: &Component, place: Place, lin: Lin) {
    let (top, below) = junction_nodes(c, place);
    let mut put = |p: Option<usize>, n: Option<usize>, g: f64, j: f64| {
        s.m(p, p, g);
        s.m(n, n, g);
        s.m(p, n, -g);
        s.m(n, p, -g);
        s.b(p, -j);
        s.b(n, j);
    };
    let linear = lin.i - lin.g * lin.v;
    match &c.kind {
        Kind::Diode(d) if d.series > 0.0 => {
            put(place.a, top, 1.0 / d.series, 0.0);
            put(top, below, lin.g, linear);
        }
        Kind::Diode(_) => put(top, below, lin.g, linear),
        Kind::Solar(m) => {
            // The photocurrent into the cell side, the diode and the shunt across it.
            let (ipv, _) = m.sources(c.state.extra[0], c.state.temperature);
            put(place.a, top, 1.0 / m.series, 0.0);
            put(top, below, lin.g + 1.0 / m.parallel, linear - ipv);
        }
        Kind::Electrolyzer(cell) => {
            put(place.a, top, 1.0 / cell.ohmic, 0.0);
            put(top, below, lin.g, linear);
        }
        _ => unreachable!("a nonlinear device"),
    }
}

/// How far the device's exact current at `new` is from its linearisation `lin` (A), and
/// that current.
pub fn error(c: &Component, place: Place, lin: Lin, new: &[f64]) -> (f64, f64) {
    let v = junction_voltage(c, place, new);
    let (i, _) = law(c, v).expect("a nonlinear device");
    ((i - (lin.i + lin.g * (v - lin.v))).abs(), i.abs())
}
