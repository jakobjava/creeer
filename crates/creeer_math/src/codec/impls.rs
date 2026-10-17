//! How the plain types are written: numbers bit for bit (decoding refuses non-finite ones),
//! counts and indices as varints, sequences with their length.

use super::{Decode, DecodeError, Encode, Reader, Writer};
use crate::glam::{DMat3, DQuat, DVec3};

impl Encode for f64 {
    fn encode(&self, w: &mut Writer) {
        w.f64(*self);
    }
}

impl Decode for f64 {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        r.finite()
    }
}

impl Encode for DVec3 {
    fn encode(&self, w: &mut Writer) {
        for v in self.to_array() {
            w.f64(v);
        }
    }
}

impl Decode for DVec3 {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self::new(r.finite()?, r.finite()?, r.finite()?))
    }
}

impl Encode for DQuat {
    fn encode(&self, w: &mut Writer) {
        for v in self.to_array() {
            w.f64(v);
        }
    }
}

impl Decode for DQuat {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self::from_xyzw(
            r.finite()?,
            r.finite()?,
            r.finite()?,
            r.finite()?,
        ))
    }
}

impl<T: Encode> Encode for [T] {
    fn encode(&self, w: &mut Writer) {
        w.varint(self.len() as u64);
        for item in self {
            item.encode(w);
        }
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, w: &mut Writer) {
        self.as_slice().encode(w);
    }
}

impl<T: Decode> Decode for Vec<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let n = r.count(1)?;
        (0..n).map(|_| T::decode(r)).collect()
    }
}

impl Encode for DMat3 {
    fn encode(&self, w: &mut Writer) {
        for v in self.to_cols_array() {
            w.f64(v);
        }
    }
}

impl Decode for DMat3 {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let mut m = [0.0; 9];
        for v in &mut m {
            *v = r.finite()?;
        }
        Ok(Self::from_cols_array(&m))
    }
}

impl<const N: usize> Encode for [f64; N] {
    fn encode(&self, w: &mut Writer) {
        for v in self {
            w.f64(*v);
        }
    }
}

impl<const N: usize> Decode for [f64; N] {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let mut out = [0.0; N];
        for v in &mut out {
            *v = r.finite()?;
        }
        Ok(out)
    }
}

impl Encode for bool {
    fn encode(&self, w: &mut Writer) {
        w.u8(u8::from(*self));
    }
}

impl Decode for bool {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(r.error("not a flag")),
        }
    }
}

impl Encode for u32 {
    fn encode(&self, w: &mut Writer) {
        w.varint(u64::from(*self));
    }
}

impl Decode for u32 {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Self::try_from(r.varint()?).map_err(|_| r.error("beyond 32 bits"))
    }
}

impl Encode for usize {
    fn encode(&self, w: &mut Writer) {
        w.varint(*self as u64);
    }
}

impl Decode for usize {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Self::try_from(r.varint()?).map_err(|_| r.error("index beyond this machine's"))
    }
}

impl<T: Encode> Encode for Option<T> {
    fn encode(&self, w: &mut Writer) {
        match self {
            None => w.u8(0),
            Some(v) => {
                w.u8(1);
                v.encode(w);
            }
        }
    }
}

impl<T: Decode> Decode for Option<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match r.u8()? {
            0 => Ok(None),
            1 => Ok(Some(T::decode(r)?)),
            _ => Err(r.error("not an option")),
        }
    }
}

impl Encode for String {
    fn encode(&self, w: &mut Writer) {
        w.bytes_with_length(self.as_bytes());
    }
}

impl Decode for String {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let bytes = r.bytes_with_length()?;
        Self::from_utf8(bytes.to_vec()).map_err(|_| r.error("text not UTF-8"))
    }
}
