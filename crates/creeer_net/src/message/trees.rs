//! Standing trees as clients are told them (G1.7, ADR-0031): which are cut into, and how far,
//! and which are felled (their stumps standing). The rest are as the forest grows them.

use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};

/// A standing tree's cut: its id, the height of its cut (m up its stem), its notches (each the
/// side it is cut from, rad about the stem, and how deep, m), and whether it is felled.
#[derive(Clone, Debug, PartialEq)]
pub struct TreeCut {
    pub id: u64,
    pub z: f64,
    pub notches: Vec<(f64, f64)>,
    pub felled: bool,
}

impl Encode for TreeCut {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.id);
        w.f64(self.z);
        w.varint(self.notches.len() as u64);
        for (side, depth) in &self.notches {
            w.f64(*side);
            w.f64(*depth);
        }
        w.u8(u8::from(self.felled));
    }
}

impl Decode for TreeCut {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let (id, z) = (r.u64()?, r.finite()?);
        let notches = (0..r.count(16)?)
            .map(|_| Ok((r.finite()?, r.finite()?)))
            .collect::<Result<_, DecodeError>>()?;
        let felled = match r.u8()? {
            0 => false,
            1 => true,
            _ => return Err(r.error("not a flag")),
        };
        Ok(Self {
            id,
            z,
            notches,
            felled,
        })
    }
}
