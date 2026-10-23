//! An item lying at rest on the ground, as clients draw and aim at it (G1.6: small things at
//! rest are scenery, not bodies): its id, where it lies and how it is turned, the half extents
//! of its box (in its own frame), its colour, a word for it, and, if it is made of pieces, each
//! piece as drawn (G1.6: a tool is its haft and its head, not one box).

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
    pub parts: Vec<Part>,
}

/// A piece of a made thing as drawn: where it sits in the thing's frame (about its centre), how
/// it is turned, its box's half extents, and its colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Part {
    pub at: DVec3,
    pub turn: DQuat,
    pub half: DVec3,
    pub colour: [u8; 3],
}

impl Encode for Part {
    fn encode(&self, w: &mut Writer) {
        w.put(&self.at);
        w.put(&self.turn);
        w.put(&self.half);
        w.raw(&self.colour);
    }
}

impl Decode for Part {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let (at, turn, half) = (r.get()?, r.get()?, r.get()?);
        let c = r.raw(3)?;
        Ok(Self {
            at,
            turn,
            half,
            colour: [c[0], c[1], c[2]],
        })
    }
}

impl Encode for Laid {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.id);
        w.put(&self.centre);
        w.put(&self.turn);
        w.put(&self.half);
        w.raw(&self.colour);
        text(w, &self.word);
        w.put(&self.parts);
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
            parts: r.get()?,
        })
    }
}
