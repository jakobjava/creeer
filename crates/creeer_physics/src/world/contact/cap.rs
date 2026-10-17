//! How stiff a contact may be in a step (`mod.rs`), as a function of the force it carries.

use creeer_math::det::{exp, ln};

/// How stiff a contact may be in a step, by the force F it carries:
///
/// - `limit` (N/m) up to F* = `limit`·`reach`, the weight of the bodies it joins (it then
///   sinks `reach` into them);
/// - F/`reach` up to `carried`, the most it can carry at rest (the weight of everything it
///   holds up, and the loads on it): each e-fold of load adds `reach` to its depth, so a
///   column or a building on its feet sinks micrometres, not millimetres;
/// - `limit` again beyond: a force no load at rest explains comes from motion (an impact, a
///   pinch) or from an overlap found late, and only a contact the step resolves keeps those
///   in bounds. (Stiffening with any force, a ball smashed into a pile stored 0.3 MJ in it
///   in one step, F5.)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cap {
    /// Stiffness up to the weight of the bodies joined (N/m).
    pub limit: f64,
    /// The depth that weight presses it in (m).
    pub reach: f64,
    /// The most it carries at rest (N).
    pub carried: f64,
}

impl Cap {
    /// No cap.
    pub const NONE: Self = Self {
        limit: f64::INFINITY,
        reach: 0.0,
        carried: 0.0,
    };

    /// Whether it caps anything.
    pub fn is_finite(&self) -> bool {
        self.limit.is_finite() && self.reach > 0.0
    }

    /// The force it stiffens from (the weight of the bodies joined) and up to (N).
    pub fn range(&self) -> (f64, f64) {
        let weight = self.limit * self.reach;
        (weight, self.carried.max(weight))
    }

    /// Its stiffness at force `force` (N/m).
    pub fn stiffness(&self, force: f64) -> f64 {
        let (from, to) = self.range();
        if force > from && force <= to {
            force / self.reach
        } else {
            self.limit
        }
    }

    /// The cap alone as a law: energy, force and stiffness at depth `depth` (any sign; it is
    /// linear below zero).
    pub fn at(&self, depth: f64) -> (f64, f64, f64) {
        let d = self.reach;
        let (from, to) = self.range();
        if depth <= d {
            let force = self.limit * depth;
            return (0.5 * force * depth, force, self.limit);
        }
        let top = d * (1.0 + ln(to / from));
        let base = 0.5 * from * d;
        if depth <= top {
            let force = from * exp((depth - d) / d);
            return (base + (force - from) * d, force, force / d);
        }
        let beyond = depth - top;
        let energy = base + (to - from) * d + to * beyond + 0.5 * self.limit * beyond * beyond;
        (energy, to + self.limit * beyond, self.limit)
    }
}

/// How many radians a contact may vibrate through in one step at most: Ω = ω·h. Real
/// contacts between hard bodies vibrate at kilohertz, far faster than a 60 Hz step resolves;
/// beyond ω·h of a few every contact behaves alike (as rigid), and more stiffness only
/// magnifies the step's errors and slows the solve. An implicit step loses energy only
/// while the energy is convex in the positions, and a contact's force through a turning
/// lever arm is not: its error grows as force × arm × (turn per step)². Steel crates
/// pressing their faces together at the flat punch's 1e11 N/m blew a pile up (58 kJ to
/// 37 MJ in a step). So a contact is at most m·(Ω/h)² stiff, m the effective mass of the
/// bodies it joins. Ω = 15: its period is under half a step, it sags g·h²/Ω² = 12 µm under
/// its weight at 60 Hz, and it is as stiff as the old 1e8 N/m penalty was for a
/// structure's joint (Ω = 45, the old penalty's stiffness for 100 kg crates, was measured
/// too: no more accurate where it mattered, and slower where structures rest on contacts).
/// Under more than the weight of the bodies it joins, the cap stiffens with the load it
/// carries at rest ([`Cap`]): a column of thirty crates sinks 1.5 mm (10.9 mm with a fixed
/// cap), a 10 t block on a 100 kg crate 0.14 mm (2.45 mm), and a loaded frame stands on
/// its joints at exactly Hertz's depth (F5). Soft contacts (rubber, soil) and the impact
/// substeps of crushing structures stay below it and follow their physical law.
const RESOLVED: f64 = 15.0;

/// The stiffest a contact of effective mass `mass` (kg) is in steps of `h` seconds (N/m).
pub fn stiffest(mass: f64, h: f64) -> f64 {
    mass * (RESOLVED / h) * (RESOLVED / h)
}

/// Standard gravity (m/s²): the weight the resolvable stiffness is measured against.
const STANDARD_GRAVITY: f64 = 9.806_65;

/// How deep a contact at the stiffest of [`stiffest`] sinks under the weight of the bodies
/// it joins in steps of `h` seconds, g·h²/Ω² (m; 12 µm at 60 Hz).
pub fn reach(h: f64) -> f64 {
    STANDARD_GRAVITY * (h / RESOLVED) * (h / RESOLVED)
}

/// The cap of a contact of effective mass `mass` (kg) in steps of `h` seconds, carrying at
/// most `carried` (N) at rest.
pub fn cap(mass: f64, h: f64, carried: f64) -> Cap {
    Cap {
        limit: stiffest(mass, h),
        reach: reach(h),
        carried,
    }
}
