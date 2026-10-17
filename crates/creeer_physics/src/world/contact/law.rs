//! How hard two surfaces push back when pressed into each other (penetration δ > 0).
//!
//! - **Points** (balls, joints, rounded edges and corners): Hertz's law for a sphere of the
//!   effective radius R on a flat, F = k·δ^{3/2} with k = 4/3·E*·√R (Johnson, *Contact
//!   Mechanics*, 1985, §4.2). Stored energy 2/5·k·δ^{5/2}, stiffness 3/2·k·√δ.
//! - **Patches** (a box's face on a floor, a wall, another box): the pressure of a flat punch
//!   over the area where the face is pressed in (`patch.rs`).
//!
//! The laws are elastic. Local yielding where real contacts are sharp would dissipate some
//! of an impact's energy; that loss is the restitution coefficient's (`impact.rs`).

/// Faces turned towards a plane by a smaller cosine (an area seen along the normal under
/// 0.1 % of their own) take no part; up to twice this they enter gradually (`Patch::new`).
pub const FACING: f64 = 1e-3;

use super::cap::Cap;
use creeer_math::det::{cbrt, exp, ln};

/// Hertz's law of a point contact, F = k·δ^{3/2}, no stiffer than the cap (`cap.rs`): at
/// every force its stiffness is the smaller of Hertz's, 3/2·k^{2/3}·F^{1/3}, and the cap's.
/// Hertz's stiffness grows as F^{1/3} and the cap's is constant or grows as F, so they
/// cross at forces known in closed form, and between them the law is Hertz's, linear, or
/// exponential (each e-fold of force a reach deeper). A contact the step resolves (a rubber
/// ball, a loaded joint on concrete) is Hertzian throughout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hertz {
    /// k = 4/3·E*·√R (N/m^{3/2}).
    pub k: f64,
    /// The effective radius R (m): the circle it touches over has radius √(R·δ).
    pub radius: f64,
    /// The cap.
    pub cap: Cap,
    /// From the force where each segment starts: (force, depth, ∫δ dF up to it, law).
    segments: [(f64, f64, f64, Piece); 5],
    /// How many segments there are.
    count: usize,
}

/// The law of a segment.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Piece {
    /// Hertz's own.
    Hertz,
    /// The cap's limit.
    Linear,
    /// The cap's stiffening, dF/dδ = F/reach.
    Stiffening,
}

impl Hertz {
    /// The law for contact modulus `modulus` (Pa) and effective radius `radius` (m).
    pub fn new(modulus: f64, radius: f64) -> Self {
        Self {
            k: 4.0 / 3.0 * modulus * radius.sqrt(),
            radius,
            cap: Cap::NONE,
            segments: [(0.0, 0.0, 0.0, Piece::Hertz); 5],
            count: 1,
        }
    }

    /// Hertz's stiffness at force `force`.
    fn own_stiffness(&self, force: f64) -> f64 {
        1.5 * cbrt(self.k * self.k * force)
    }

    /// Hertz's depth at force `force`.
    fn own_depth(&self, force: f64) -> f64 {
        let root = cbrt(force / self.k);
        root * root
    }

    /// Sets the cap, and with it the segments.
    pub fn set_cap(&mut self, cap: Cap) {
        if cap == self.cap {
            return;
        }
        self.cap = cap;
        self.count = 1;
        self.segments[0] = (0.0, 0.0, 0.0, Piece::Hertz);
        if !cap.is_finite() {
            return;
        }
        let (from, to) = cap.range();
        // Where Hertz's stiffness meets the limit, and meets F/reach.
        let meets_limit = {
            let x = cap.limit / (1.5 * cbrt(self.k * self.k));
            x * x * x
        };
        let meets_growth = {
            let x = 1.5 * cap.reach;
            x * x.sqrt() * self.k
        };
        let mut breaks = [meets_limit, from, meets_growth, to];
        breaks.sort_by(f64::total_cmp);
        let mut previous = 0.0;
        for b in breaks {
            if b <= previous || !b.is_finite() {
                continue;
            }
            self.close(previous, b);
            previous = b;
        }
        self.close(previous, f64::INFINITY);
    }

    /// Ends the last segment at force `end` if the law changes there (the next starts at
    /// `start`, deciding its piece at a force inside (`start`, `end`)).
    fn close(&mut self, start: f64, end: f64) {
        let inside = if end.is_finite() {
            0.5 * (start + end)
        } else {
            2.0 * start + 1.0
        };
        let (from, to) = self.cap.range();
        let piece = if self.own_stiffness(inside) <= self.cap.stiffness(inside) {
            Piece::Hertz
        } else if inside > from && inside <= to {
            Piece::Stiffening
        } else {
            Piece::Linear
        };
        let last = self.segments[self.count - 1];
        if piece == last.3 {
            return;
        }
        let (depth, integral) = self.through(last, start);
        self.segments[self.count] = (start, depth, integral, piece);
        self.count += 1;
    }

    /// Depth and ∫δ dF at force `force` within `segment`.
    fn through(&self, (f0, d0, c0, piece): (f64, f64, f64, Piece), force: f64) -> (f64, f64) {
        let span = force - f0;
        match piece {
            Piece::Hertz => {
                let shift = d0 - self.own_depth(f0);
                let five = |f: f64| 0.6 * f * self.own_depth(f);
                (
                    shift + self.own_depth(force),
                    c0 + shift * span + five(force) - five(f0),
                )
            }
            Piece::Linear => (
                d0 + span / self.cap.limit,
                c0 + d0 * span + 0.5 * span * span / self.cap.limit,
            ),
            Piece::Stiffening => {
                let d = self.cap.reach;
                let lift = ln(force / f0);
                (d0 + d * lift, c0 + d0 * span + d * (force * lift - span))
            }
        }
    }

    /// Energy, force and stiffness at penetration `depth`.
    fn law(&self, depth: f64) -> (f64, f64, f64) {
        if depth <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let i = (1..self.count)
            .rev()
            .find(|&i| self.segments[i].1 <= depth)
            .unwrap_or(0);
        let (f0, d0, _, piece) = self.segments[i];
        let (force, stiffness) = match piece {
            Piece::Hertz => {
                let x = depth - d0 + self.own_depth(f0);
                let force = self.k * x * x.sqrt();
                (force, self.own_stiffness(force))
            }
            Piece::Linear => (f0 + self.cap.limit * (depth - d0), self.cap.limit),
            Piece::Stiffening => {
                let force = f0 * exp((depth - d0) / self.cap.reach);
                (force, force / self.cap.reach)
            }
        };
        let (_, integral) = self.through(self.segments[i], force);
        (force * depth - integral, force, stiffness)
    }

    /// Stored energy at penetration `depth` (J).
    pub fn energy(&self, depth: f64) -> f64 {
        self.law(depth).0
    }

    /// Normal force at penetration `depth` (N).
    pub fn force(&self, depth: f64) -> f64 {
        self.law(depth).1
    }

    /// Stiffness dF/dδ at penetration `depth` (N/m).
    pub fn stiffness(&self, depth: f64) -> f64 {
        self.law(depth).2
    }

    /// The penetration at which it carries `force` (m).
    pub fn depth(&self, force: f64) -> f64 {
        let force = force.max(0.0);
        let i = (1..self.count)
            .rev()
            .find(|&i| self.segments[i].0 <= force)
            .unwrap_or(0);
        self.through(self.segments[i], force).0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A law with every piece: Hertz's up to 30 N, the limit to 1 kN (the weight joined),
    /// stiffening to 5.8 kN, Hertz's again to 1 MN (the most carried at rest), the limit
    /// beyond.
    fn capped() -> Hertz {
        let mut law = Hertz::new(75.0e9, 1.0);
        law.set_cap(Cap {
            limit: 1.0e8,
            reach: 1.0e-5,
            carried: 1.0e6,
        });
        law
    }

    #[test]
    fn the_capped_law_is_consistent_through_all_its_pieces() {
        let law = capped();
        assert_eq!(law.count, 5, "{:?}", law.segments);
        let mut depth: f64 = 1e-8;
        while depth < 1e-1 {
            let e = depth * 1e-6;
            let fd = (law.energy(depth + e) - law.energy(depth - e)) / (2.0 * e);
            let force = law.force(depth);
            assert!(
                (fd / force - 1.0).abs() < 1e-5,
                "{depth:e}: {fd} vs {force}"
            );
            let kd = (law.force(depth + e) - law.force(depth - e)) / (2.0 * e);
            let k = law.stiffness(depth);
            assert!((kd / k - 1.0).abs() < 1e-4, "{depth:e}: {kd} vs {k}");
            assert!((law.depth(force) / depth - 1.0).abs() < 1e-9, "{depth:e}");
            // Never stiffer than Hertz's law or the cap at that force.
            let own = 1.5 * cbrt(law.k * law.k * force);
            assert!(k <= own * (1.0 + 1e-9) && k <= law.cap.stiffness(force) * (1.0 + 1e-9));
            depth *= 1.37;
        }
    }

    /// Each e-fold of force between the weight joined and the most carried at rest adds the
    /// reach to the depth.
    #[test]
    fn a_stiffening_cap_adds_a_reach_per_e_fold() {
        let law = capped();
        let d = law.cap.reach;
        let step = law.depth(2000.0 * std::f64::consts::E) - law.depth(2000.0);
        assert!((step / d - 1.0).abs() < 1e-9, "{step:e}");
    }

    #[test]
    fn hertz_force_is_the_energy_slope() {
        let law = Hertz::new(100.0e9, 0.05);
        for depth in [1e-6, 1e-5, 3e-4] {
            let e = depth * 1e-6;
            let fd = (law.energy(depth + e) - law.energy(depth - e)) / (2.0 * e);
            assert!((fd / law.force(depth) - 1.0).abs() < 1e-6);
            let kd = (law.force(depth + e) - law.force(depth - e)) / (2.0 * e);
            assert!((kd / law.stiffness(depth) - 1.0).abs() < 1e-6);
            assert!((law.depth(law.force(depth)) / depth - 1.0).abs() < 1e-12);
        }
    }
}
