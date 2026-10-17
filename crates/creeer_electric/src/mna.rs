//! Modified nodal analysis: an island's unknowns (its nodes' voltages, then the extra
//! unknowns some components bring) and every component's stamp, linearised at Newton's
//! current iterate and discretised by the step's integration rule.
//!
//! One stamping routine serves twice: once to record the matrix's pattern (which entries
//! exist, in which order), then every iteration to write their values in that same order.
//! Nothing it emits depends on the values (an open switch writes zeros), so the pattern is
//! fixed and the LU's pivots can be reused.

use crate::component::{CONTACT, Component, GROUND, Kind};
use crate::nonlinear;

/// Leakage from every node to ground (S): SPICE's GMIN, so that a node an open switch cuts
/// off is still defined (1 TΩ: no circuit notices).
pub const GMIN: f64 = 1e-12;

/// Where a component's unknowns are: its terminals' (None for ground), its extra unknowns'
/// first index.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    pub a: Option<usize>,
    pub b: Option<usize>,
    pub extra: usize,
}

/// How many extra unknowns a component brings: a voltage source's, a battery's and an
/// inductor-free branch's current; a machine's armature current (its rotor eliminated); a diode's
/// or a solar module's internal node (behind its series resistance).
pub fn extras(c: &Component) -> usize {
    match &c.kind {
        Kind::VoltageSource(_) | Kind::Battery(_) => 1,
        Kind::Machine(_) => 1,
        Kind::Diode(d) if d.series > 0.0 => 1,
        Kind::Solar(_) | Kind::Electrolyzer(_) => 1,
        _ => 0,
    }
}

/// The integration rule: d/dt x ≈ (a0·x₊ + a1·x₀ + a2·x₋)/h (backward Euler: 1, −1, 0;
/// BDF2: 3/2, −2, 1/2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rule {
    pub a0: f64,
    pub a1: f64,
    pub a2: f64,
    pub h: f64,
}

impl Rule {
    /// Backward Euler with step `h`.
    pub fn euler(h: f64) -> Self {
        Self {
            a0: 1.0,
            a1: -1.0,
            a2: 0.0,
            h,
        }
    }

    /// BDF2 with step `h`.
    pub fn bdf2(h: f64) -> Self {
        Self {
            a0: 1.5,
            a1: -2.0,
            a2: 0.5,
            h,
        }
    }

    /// The history part of d/dt x: (a1·x₀ + a2·x₋)/h.
    pub fn history(&self, now: f64, before: f64) -> f64 {
        (self.a1 * now + self.a2 * before) / self.h
    }
}

/// Where stamps go.
pub trait Sink {
    /// Adds `v` to the matrix at (row, column) (nothing where either is ground).
    fn m(&mut self, r: Option<usize>, c: Option<usize>, v: f64);
    /// Adds `v` to the right-hand side at `r`.
    fn b(&mut self, r: Option<usize>, v: f64);
}

/// Records the pattern.
#[derive(Default)]
pub struct Pattern(pub Vec<(usize, usize)>);

impl Sink for Pattern {
    fn m(&mut self, r: Option<usize>, c: Option<usize>, _: f64) {
        if let (Some(r), Some(c)) = (r, c) {
            self.0.push((r, c));
        }
    }
    fn b(&mut self, _: Option<usize>, _: f64) {}
}

/// Writes values in the pattern's order, and the right-hand side.
pub struct Values<'a> {
    pub values: &'a mut Vec<f64>,
    pub rhs: &'a mut [f64],
}

impl Sink for Values<'_> {
    fn m(&mut self, r: Option<usize>, c: Option<usize>, v: f64) {
        if r.is_some() && c.is_some() {
            self.values.push(v);
        }
    }
    fn b(&mut self, r: Option<usize>, v: f64) {
        if let Some(r) = r {
            self.rhs[r] += v;
        }
    }
}

/// Writes the right-hand side alone (a component whose matrix entries have not changed).
pub struct RhsOnly<'a>(pub &'a mut [f64]);

impl Sink for RhsOnly<'_> {
    fn m(&mut self, _: Option<usize>, _: Option<usize>, _: f64) {}
    fn b(&mut self, r: Option<usize>, v: f64) {
        if let Some(r) = r {
            self.0[r] += v;
        }
    }
}

/// Records a component's right-hand side (its few entries), to be added later in order: the
/// components' stamps worked out side by side, their sums the same bits as one after another.
#[derive(Clone, Copy, Debug, Default)]
pub struct Recorded {
    pub entries: [(usize, f64); 4],
    pub len: usize,
}

impl Sink for Recorded {
    fn m(&mut self, _: Option<usize>, _: Option<usize>, _: f64) {}
    fn b(&mut self, r: Option<usize>, v: f64) {
        if let Some(r) = r {
            self.entries[self.len] = (r, v);
            self.len += 1;
        }
    }
}

/// Overwrites one component's matrix entries in place from `at` (their offset in the
/// pattern), noting whether any changed, and adds its right-hand side.
pub struct At<'a> {
    pub values: &'a mut [f64],
    pub at: usize,
    pub rhs: &'a mut [f64],
    pub changed: bool,
}

impl Sink for At<'_> {
    fn m(&mut self, r: Option<usize>, c: Option<usize>, v: f64) {
        if r.is_some() && c.is_some() {
            #[allow(clippy::float_cmp)] // any change at all
            let same = self.values[self.at] == v;
            self.changed |= !same;
            self.values[self.at] = v;
            self.at += 1;
        }
    }
    fn b(&mut self, r: Option<usize>, v: f64) {
        if let Some(r) = r {
            self.rhs[r] += v;
        }
    }
}

/// A conductance `g` between `a` and `b`, and a current `j` driven through it from a to b.
fn branch<S: Sink>(s: &mut S, a: Option<usize>, b: Option<usize>, g: f64, j: f64) {
    s.m(a, a, g);
    s.m(b, b, g);
    s.m(a, b, -g);
    s.m(b, a, -g);
    s.b(a, -j);
    s.b(b, j);
}

/// The voltage across (a, b) at `x`.
pub fn across(x: &[f64], a: Option<usize>, b: Option<usize>) -> f64 {
    a.map_or(0.0, |i| x[i]) - b.map_or(0.0, |i| x[i])
}

/// A resistor's conductance, a wire's at its temperature, a contact's when closed.
pub fn conductance(c: &Component) -> Option<f64> {
    match &c.kind {
        Kind::Resistor(r) => Some(1.0 / r),
        Kind::Wire {
            resistivity,
            length,
            area,
        } => Some(area / (resistivity.at(c.state.temperature) * length)),
        Kind::Switch | Kind::Fuse { .. } | Kind::Breaker { .. } => {
            Some(if c.state.closed { 1.0 / CONTACT } else { 0.0 })
        }
        _ => None,
    }
}

/// Stamps component `c` at `place`, linearised at `x`, with rule `rule`.
#[allow(clippy::too_many_lines)] // one arm per kind of component
pub fn stamp<S: Sink>(s: &mut S, c: &Component, place: Place, x: &[f64], rule: Rule) {
    let (a, b, e) = (place.a, place.b, place.extra);
    let st = &c.state;
    match &c.kind {
        Kind::Resistor(_)
        | Kind::Wire { .. }
        | Kind::Switch
        | Kind::Fuse { .. }
        | Kind::Breaker { .. } => {
            branch(s, a, b, conductance(c).expect("a conductance"), 0.0);
        }
        Kind::Capacitor(farads) => {
            // i = C·(a0·v + a1·v₀ + a2·v₋)/h.
            let g = farads * rule.a0 / rule.h;
            branch(s, a, b, g, farads * rule.history(st.v[0], st.v[1]));
        }
        Kind::Inductor(henries) => {
            // v = L·(a0·i + a1·i₀ + a2·i₋)/h, so i = h/(a0·L)·v − (a1·i₀ + a2·i₋)/a0.
            let g = rule.h / (rule.a0 * henries);
            branch(
                s,
                a,
                b,
                g,
                -(rule.a1 * st.i[0] + rule.a2 * st.i[1]) / rule.a0,
            );
        }
        Kind::VoltageSource(volts) => {
            // The branch current from a to b is unknown `e`; v_a − v_b = V.
            s.m(a, Some(e), 1.0);
            s.m(b, Some(e), -1.0);
            s.m(Some(e), a, 1.0);
            s.m(Some(e), b, -1.0);
            s.m(Some(e), Some(e), 0.0);
            s.b(Some(e), *volts);
        }
        Kind::CurrentSource(amps) => branch(s, a, b, 0.0, *amps),
        Kind::Diode(_) | Kind::Solar(_) | Kind::Electrolyzer(_) => {
            nonlinear::stamp(s, c, place, nonlinear::linearise(c, place, x))
        }
        Kind::Battery(bat) => {
            // v_a − v_b = OCV + (R₀ + α)·i + β with the RC pair's voltage v₁ = α·i + β
            // (C₁·dv₁/dt = i − v₁/R₁, discretised). Disconnected: i = 0.
            let k = bat.c1 * rule.a0 / rule.h + 1.0 / bat.r1;
            let (alpha, beta) = (
                1.0 / k,
                -bat.c1 * rule.history(st.extra[0], st.extra[1]) / k,
            );
            let on = if st.closed { 1.0 } else { 0.0 };
            s.m(a, Some(e), 1.0);
            s.m(b, Some(e), -1.0);
            s.m(Some(e), a, on);
            s.m(Some(e), b, -on);
            s.m(
                Some(e),
                Some(e),
                if st.closed { -(bat.r0 + alpha) } else { 1.0 },
            );
            s.b(Some(e), on * (bat.open_circuit(st.level) + beta));
        }
        Kind::Machine(m) => {
            // Armature: v_a − v_b = R·i + L·(a0·i + hist)/h + k·ω.
            // Rotor: ω = free + mobility·k·i (`rotor`): eliminated, reflected into the armature
            // as k²·mobility (its speed found after the solve).
            let (free, mobility) = rotor(m, st, rule);
            let i = Some(e);
            s.m(a, i, 1.0);
            s.m(b, i, -1.0);
            s.m(i, a, 1.0);
            s.m(i, b, -1.0);
            s.m(
                i,
                i,
                -(m.resistance
                    + m.inductance * rule.a0 / rule.h
                    + m.constant * m.constant * mobility),
            );
            s.b(
                i,
                m.inductance * rule.history(st.i[0], st.i[1]) + m.constant * free,
            );
        }
    }
}

/// Whether `c` makes its island nonlinear (Newton must iterate).
pub fn nonlinear(c: &Component) -> bool {
    matches!(
        c.kind,
        Kind::Diode(_) | Kind::Solar(_) | Kind::Electrolyzer(_)
    )
}

/// The global terminal nodes as local unknowns.
pub fn terminals(c: &Component, local: &[Option<usize>]) -> (Option<usize>, Option<usize>) {
    let at = |n: usize| if n == GROUND { None } else { local[n] };
    (at(c.a), at(c.b))
}

/// Whether `c` adds to the right-hand side when its matrix entries are kept: its history
/// (capacitors, inductors, batteries, machines) or its value (sources).
pub fn has_rhs(c: &Component) -> bool {
    matches!(
        c.kind,
        Kind::Capacitor(_)
            | Kind::Inductor(_)
            | Kind::VoltageSource(_)
            | Kind::CurrentSource(_)
            | Kind::Battery(_)
            | Kind::Machine(_)
    )
}

/// A machine's rotor equation by rule `rule`, ω = free + mobility·k·i: (free, mobility).
/// Its own rotor: J·(a0·ω + hist)/h = k·i − b·ω − load. On a drivetrain: the drivetrain's.
pub fn rotor(
    m: &crate::devices::DcMachine,
    st: &crate::component::State,
    rule: Rule,
) -> (f64, f64) {
    if let Some(shaft) = st.shaft {
        return shaft;
    }
    let d = m.inertia * rule.a0 / rule.h + m.friction;
    (
        (-m.inertia * rule.history(st.level, st.extra[0]) - st.load) / d,
        1.0 / d,
    )
}
