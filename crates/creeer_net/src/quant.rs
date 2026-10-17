//! Quantised states: what a snapshot sends of a body, at the precision a client needs.
//!
//! - **Positions** to 1/1024 m (under a millimetre: a crate is drawn where it is to the
//!   pixel at a few metres), as integers in the physics world's frame: a 500 km planet's
//!   surface is within ±2³¹ of them.
//! - **Orientations** as the three smallest components of the unit quaternion (the largest
//!   follows from them, its sign made positive), 65,535 steps each over ±1/√2: 2·10⁻⁵ of a
//!   component, an angle of about 4·10⁻⁵ rad, in seven bytes.
//! - **Velocities** to 1/256 m/s and rad/s: for interpolating between snapshots and running
//!   a body on past the last one.
//!
//! Quantising is idempotent (a quantised state quantises to itself), so the server compares
//! what it would send with what the client has bit for bit, and a body at rest costs nothing.

use creeer_math::codec::{DecodeError, Reader, Writer};
use creeer_math::glam::{DQuat, DVec3};

/// Positions per metre.
pub const POSITION_SCALE: f64 = 1024.0;
/// Velocities per m/s (and angular velocities per rad/s).
pub const VELOCITY_SCALE: f64 = 256.0;
/// Half the steps of each of the three smallest quaternion components over [−1/√2, 1/√2].
const ROTATION_STEPS: f64 = 32_767.0;

/// A vector quantised to integers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Quantised(pub [i64; 3]);

impl Quantised {
    /// `v` in steps of 1/`scale`.
    pub fn new(v: DVec3, scale: f64) -> Self {
        #[allow(clippy::cast_possible_truncation)] // within ±2⁶³ for any position we use
        Self(v.to_array().map(|x| (x * scale).round() as i64))
    }

    /// The vector, in steps of 1/`scale`.
    pub fn value(self, scale: f64) -> DVec3 {
        #[allow(clippy::cast_precision_loss)] // exact below 2⁵³ steps
        DVec3::from_array(self.0.map(|q| q as f64 / scale))
    }

    /// Writes it as three signed varints, less `base` (small for a body that moved little).
    pub fn write(self, w: &mut Writer, base: Self) {
        for (q, b) in self.0.iter().zip(base.0) {
            w.signed(q.wrapping_sub(b));
        }
    }

    /// Reads one written by [`Self::write`] against `base`.
    ///
    /// # Errors
    /// If the bytes end too soon.
    pub fn read(r: &mut Reader<'_>, base: Self) -> Result<Self, DecodeError> {
        let mut out = [0; 3];
        for (o, b) in out.iter_mut().zip(base.0) {
            *o = r.signed()?.wrapping_add(b);
        }
        Ok(Self(out))
    }
}

/// An orientation quantised as its three smallest components (a 47-bit word).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rotation(pub u64);

impl Rotation {
    /// `q` (normalised first), its largest component made positive.
    pub fn new(q: DQuat) -> Self {
        let q = q.normalize();
        let c = q.to_array();
        let largest = (0..4)
            .max_by(|&a, &b| c[a].abs().total_cmp(&c[b].abs()))
            .expect("four components");
        let sign = if c[largest] < 0.0 { -1.0 } else { 1.0 };
        let mut word = largest as u64;
        let limit = std::f64::consts::FRAC_1_SQRT_2;
        for k in (0..4).filter(|&k| k != largest) {
            let v = (c[k] * sign).clamp(-limit, limit) / limit;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=65534
            let step = ((v + 1.0) * ROTATION_STEPS).round() as u64;
            word = (word << 16) | step;
        }
        Self(word)
    }

    /// The orientation.
    pub fn value(self) -> DQuat {
        let limit = std::f64::consts::FRAC_1_SQRT_2;
        #[allow(clippy::cast_possible_truncation)] // two bits
        let largest = ((self.0 >> 48) & 3) as usize;
        let mut c = [0.0; 4];
        let mut shift = 32;
        for k in (0..4).filter(|&k| k != largest) {
            #[allow(clippy::cast_precision_loss)] // 16 bits
            let step = ((self.0 >> shift) & 0xffff) as f64;
            c[k] = (step / ROTATION_STEPS - 1.0) * limit;
            shift -= 16;
        }
        let rest: f64 = c.iter().map(|v| v * v).sum();
        c[largest] = (1.0 - rest).max(0.0).sqrt();
        DQuat::from_array(c).normalize()
    }

    /// Writes it (seven bytes: two bits of index, three 16-bit fields).
    pub fn write(self, w: &mut Writer) {
        for k in 0..7 {
            #[allow(clippy::cast_possible_truncation)] // one byte of it
            w.u8((self.0 >> (8 * k)) as u8);
        }
    }

    /// Reads one written by [`Self::write`].
    ///
    /// # Errors
    /// If the bytes end too soon, or it holds bits no rotation has.
    pub fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let mut word = 0u64;
        for k in 0..7 {
            word |= u64::from(r.u8()?) << (8 * k);
        }
        if word >> 50 != 0 {
            return Err(r.error("not a rotation"));
        }
        Ok(Self(word))
    }
}

/// What a snapshot holds of a body (a node of it): where it is, how it is turned and how it
/// moves, quantised.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BodyState {
    /// Position (1/1024 m).
    pub position: Quantised,
    /// Orientation.
    pub rotation: Rotation,
    /// Velocity (1/256 m/s).
    pub velocity: Quantised,
    /// Angular velocity (1/256 rad/s).
    pub spin: Quantised,
    /// Whether it sleeps (rests, and stays put until something wakes it).
    pub asleep: bool,
}

impl BodyState {
    /// The state of a body at `position`, turned by `orientation`, moving at `velocity` and
    /// `spin`.
    pub fn new(
        position: DVec3,
        orientation: DQuat,
        (velocity, spin): (DVec3, DVec3),
        asleep: bool,
    ) -> Self {
        Self {
            position: Quantised::new(position, POSITION_SCALE),
            rotation: Rotation::new(orientation),
            velocity: Quantised::new(velocity, VELOCITY_SCALE),
            spin: Quantised::new(spin, VELOCITY_SCALE),
            asleep,
        }
    }

    /// Position (m).
    pub fn position(&self) -> DVec3 {
        self.position.value(POSITION_SCALE)
    }

    /// Orientation.
    pub fn orientation(&self) -> DQuat {
        self.rotation.value()
    }

    /// Velocity (m/s).
    pub fn velocity(&self) -> DVec3 {
        self.velocity.value(VELOCITY_SCALE)
    }

    /// Angular velocity (rad/s).
    pub fn spin(&self) -> DVec3 {
        self.spin.value(VELOCITY_SCALE)
    }
}

#[cfg(test)]
mod tests;
