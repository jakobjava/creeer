//! What a wire carries: a boolean or a number.

use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};

/// A signal on a wire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Signal {
    Bool(bool),
    Number(f64),
}

impl Default for Signal {
    /// An unconnected input reads false (zero).
    fn default() -> Self {
        Self::Bool(false)
    }
}

impl Signal {
    /// As a condition: a boolean as it is, a number when not zero (NaN is false).
    pub fn truth(self) -> bool {
        match self {
            Self::Bool(b) => b,
            Self::Number(x) => x != 0.0 && !x.is_nan(),
        }
    }

    /// As a number: true is 1, false 0.
    pub fn number(self) -> f64 {
        match self {
            Self::Bool(b) => f64::from(u8::from(b)),
            Self::Number(x) => x,
        }
    }

    /// The bits of the signal, for hashing a run.
    pub fn bits(self) -> u64 {
        match self {
            Self::Bool(b) => u64::from(b) | 1 << 63,
            Self::Number(x) => x.to_bits(),
        }
    }
}

impl From<bool> for Signal {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<f64> for Signal {
    fn from(x: f64) -> Self {
        Self::Number(x)
    }
}

impl Encode for Signal {
    fn encode(&self, w: &mut Writer) {
        match *self {
            Self::Bool(b) => w.u8(u8::from(b)),
            Self::Number(x) => {
                w.u8(2);
                w.f64(x);
            }
        }
    }
}

impl Decode for Signal {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            0 => Self::Bool(false),
            1 => Self::Bool(true),
            2 => Self::Number(r.f64()?),
            _ => return Err(r.error("not a signal")),
        })
    }
}
