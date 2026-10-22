//! What a player carries, as their client shows it (G1.6a, ADR-0028): each pocket and what is
//! in it, as the server describes it (`creeer_sim::items::kit::describe`: what can be seen of
//! it, not what only an assay would tell).

use super::{read_text, text};
use crate::entity::EntityId;
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};

/// An item as shown: what it looks like, its colour (sRGB, if it has one: G1.6c), its mass (kg)
/// and the room it takes (m³).
#[derive(Clone, Debug, PartialEq)]
pub struct ItemView {
    pub text: String,
    pub colour: Option<[u8; 3]>,
    pub mass: f64,
    pub room: f64,
}

/// A store as shown: its name, its volume (m³), and what is in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StoreView {
    pub name: String,
    pub volume: f64,
    pub items: Vec<ItemView>,
}

/// A player's inventory: their pockets, and the container they look into, if any.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inventory {
    pub pockets: Vec<StoreView>,
    pub open: Option<(EntityId, StoreView)>,
}

impl Encode for ItemView {
    fn encode(&self, w: &mut Writer) {
        text(w, &self.text);
        match self.colour {
            Some(c) => {
                w.u8(1);
                w.raw(&c);
            }
            None => w.u8(0),
        }
        w.f64(self.mass);
        w.f64(self.room);
    }
}

impl Decode for ItemView {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            text: read_text(r)?,
            colour: match r.u8()? {
                0 => None,
                1 => {
                    let c = r.raw(3)?;
                    Some([c[0], c[1], c[2]])
                }
                _ => return Err(r.error("not a flag")),
            },
            mass: r.finite()?,
            room: r.finite()?,
        })
    }
}

impl Encode for StoreView {
    fn encode(&self, w: &mut Writer) {
        text(w, &self.name);
        w.f64(self.volume);
        w.put(&self.items);
    }
}

impl Decode for StoreView {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            name: read_text(r)?,
            volume: r.finite()?,
            items: r.get()?,
        })
    }
}

impl Encode for Inventory {
    fn encode(&self, w: &mut Writer) {
        w.put(&self.pockets);
        match &self.open {
            Some((id, store)) => {
                w.u8(1);
                w.put(id);
                w.put(store);
            }
            None => w.u8(0),
        }
    }
}

impl Decode for Inventory {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let pockets = r.get()?;
        let open = match r.u8()? {
            0 => None,
            1 => Some((r.get()?, r.get()?)),
            _ => return Err(r.error("not a flag")),
        };
        Ok(Self { pockets, open })
    }
}
