//! An item lying at rest on the ground, as clients draw and aim at it (G1.6: small things at
//! rest are scenery, not bodies): its id, where it lies and how it is turned, the half extents
//! of its box (in its own frame), its colour, and a word for it.

use super::{read_text, text};
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};
use creeer_math::glam::{DQuat, DVec3};

#[derive(Clone, Debug, PartialEq)]
pub struct Laid {
    pub id: u64,
    pub centre: DVec3,
    pub turn: DQuat,
    pub half: DVec3,
    pub colour: [u8; 3],
    pub word: String,
}

impl Encode for Laid {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.id);
        w.put(&self.centre);
        w.put(&self.turn);
        w.put(&self.half);
        w.raw(&self.colour);
        text(w, &self.word);
    }
}

impl Decode for Laid {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let (id, centre, turn, half) = (r.u64()?, r.get()?, r.get()?, r.get()?);
        let c = r.raw(3)?;
        Ok(Self {
            id,
            centre,
            turn,
            half,
            colour: [c[0], c[1], c[2]],
            word: read_text(r)?,
        })
    }
}
