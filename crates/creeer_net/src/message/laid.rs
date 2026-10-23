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
    /// Its mass (kg, G1.9: told as it is lifted).
    pub mass: f64,
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
        w.f64(self.mass);
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
            mass: r.finite()?,
        })
    }
}

/// What an item body is (G1.6, `ServerMessage::Item`): a word for it (by its shape, never its
/// material), the pieces it is drawn as if it is made of them (about its centre), its mass
/// (kg), and, a made tool, where a hand holds it, about its centre (G1.10: carried in one hand
/// at its balance; drawn there in the hand).
#[derive(Clone, Debug, PartialEq)]
pub struct ItemLook {
    pub word: String,
    pub parts: Vec<Part>,
    pub mass: f64,
    pub grip: Option<DVec3>,
}

impl Encode for ItemLook {
    fn encode(&self, w: &mut Writer) {
        text(w, &self.word);
        w.put(&self.parts);
        w.f64(self.mass);
        w.put(&self.grip.is_some());
        w.put(&self.grip.unwrap_or_default());
    }
}

impl Decode for ItemLook {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let (word, parts, mass) = (read_text(r)?, r.get()?, r.finite()?);
        let (held, grip): (bool, DVec3) = (r.get()?, r.get()?);
        Ok(Self {
            word,
            parts,
            mass,
            grip: held.then_some(grip),
        })
    }
}
