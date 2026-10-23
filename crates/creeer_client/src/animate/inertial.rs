//! Transitions without pops (G1.4a): inertialization (Bollo, "High Performance Animation in
//! Gears of War 4", SIGGRAPH 2017 talk; the method in his GDC 2018 talk). At a transition
//! the new pose is drawn at once, plus the old pose's offset from it, which a quintic curve
//! takes to zero: it starts at the offset and at the old pose's velocity, ends at rest, and its
//! starting acceleration is chosen so it never overshoots. Each quantity on its own, along the
//! direction of its offset (a rotation's about its axis).

use creeer_math::glam::{DQuat, DVec3};

/// How long a transition takes (s).
pub const BLEND: f64 = 0.2;

/// One quantity's decaying offset: its direction, the curve's coefficients and its length.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Curve {
    direction: DVec3,
    /// x(t) = c[0] + c[1]·t + … + c[5]·t⁵ along the direction, for t up to `length`.
    c: [f64; 6],
    length: f64,
}

impl Curve {
    /// The curve from offset `x0` (along `direction`) at velocity `v0` along it, to zero in
    /// at most `length` seconds. A velocity away from zero is dropped, and the time shortened
    /// so the offset cannot pass zero (Bollo).
    fn new(direction: DVec3, x0: f64, v0: f64, length: f64) -> Self {
        let v0 = v0.min(0.0);
        let t = if v0 < 0.0 {
            length.min(-5.0 * x0 / v0)
        } else {
            length
        };
        if t <= 1e-9 || x0 <= 0.0 {
            return Self::default();
        }
        let a0 = (-8.0 * v0 * t - 20.0 * x0) / (t * t);
        let (t2, t3) = (t * t, t * t * t);
        let (t4, t5) = (t3 * t, t3 * t2);
        let c5 = -(a0 * t2 + 6.0 * v0 * t + 12.0 * x0) / (2.0 * t5);
        let c4 = (3.0 * a0 * t2 + 16.0 * v0 * t + 30.0 * x0) / (2.0 * t4);
        let c3 = -(3.0 * a0 * t2 + 12.0 * v0 * t + 20.0 * x0) / (2.0 * t3);
        Self {
            direction,
            c: [x0, v0, 0.5 * a0, c3, c4, c5],
            length: t,
        }
    }

    /// The curve taking a vector's offset `offset` (old minus new) at `velocity` (of the old
    /// value) to zero.
    pub fn vector(offset: DVec3, velocity: DVec3, length: f64) -> Self {
        let x0 = offset.length();
        if x0 < 1e-12 {
            return Self::default();
        }
        let d = offset / x0;
        Self::new(d, x0, velocity.dot(d), length)
    }

    /// The curve taking a rotation's offset `old · new⁻¹` to none, the old rotation turning at
    /// `spin` (rad/s, an axis times a rate).
    pub fn rotation(old: DQuat, new: DQuat, spin: DVec3, length: f64) -> Self {
        let mut offset = old * new.inverse();
        // The short way round.
        if offset.w < 0.0 {
            offset = -offset;
        }
        let (axis, angle) = offset.to_axis_angle();
        if angle < 1e-9 {
            return Self::default();
        }
        Self::new(axis, angle, spin.dot(axis), length)
    }

    /// The offset `t` seconds into the transition, along its direction.
    pub fn at(&self, t: f64) -> f64 {
        if t >= self.length {
            return 0.0;
        }
        self.c.iter().rev().fold(0.0, |sum, c| sum * t + c)
    }

    /// The vector offset at `t`.
    pub fn offset(&self, t: f64) -> DVec3 {
        self.direction * self.at(t)
    }

    /// The rotation offset at `t` (to apply before the new rotation).
    pub fn turn(&self, t: f64) -> DQuat {
        let x = self.at(t);
        if x == 0.0 {
            DQuat::IDENTITY
        } else {
            DQuat::from_axis_angle(self.direction, x)
        }
    }
}

/// The spin (rad/s) taking rotation `a` to `b` in `dt`.
pub fn spin(a: DQuat, b: DQuat, dt: f64) -> DVec3 {
    let mut d = b * a.inverse();
    if d.w < 0.0 {
        d = -d;
    }
    let (axis, angle) = d.to_axis_angle();
    axis * (angle / dt.max(1e-9))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curve_starts_at_the_offset_and_its_velocity_and_ends_at_rest() {
        let c = Curve::new(DVec3::X, 0.3, -0.5, BLEND);
        let h = 1e-6;
        assert!((c.at(0.0) - 0.3).abs() < 1e-12);
        assert!(((c.at(h) - c.at(0.0)) / h + 0.5).abs() < 1e-3);
        let end = c.length;
        assert!(c.at(end - 1e-9).abs() < 1e-6);
        assert!(((c.at(end - 1e-6) - c.at(end - 2e-6)) / 1e-6).abs() < 1e-3);
    }

    #[test]
    fn a_curve_never_overshoots() {
        for (x0, v0) in [
            (0.3, -10.0),
            (0.3, -0.1),
            (0.3, 0.0),
            (0.05, -3.0),
            (1.0, 4.0),
        ] {
            let c = Curve::new(DVec3::X, x0, v0, BLEND);
            for k in 0..=1000 {
                let x = c.at(f64::from(k) / 1000.0 * BLEND);
                assert!((-1e-9..=x0 + 1e-9).contains(&x), "{x0} {v0}: {x}");
            }
        }
    }

    #[test]
    fn a_rotation_offset_turns_the_new_rotation_back_to_the_old() {
        let (old, new) = (DQuat::from_rotation_x(0.4), DQuat::from_rotation_y(0.2));
        let c = Curve::rotation(old, new, DVec3::ZERO, BLEND);
        let start = c.turn(0.0) * new;
        assert!(start.angle_between(old) < 1e-9);
        assert!((c.turn(BLEND) * new).angle_between(new) < 1e-12);
    }
}
