//! The `ITEM` section (version 10, G1.6a): the bodies that are items (`crate::items`), by their
//! entity ids, what each avatar's pockets hold, and the loose things picked up from the ground
//! (G1.6c, `crate::scatter`), in order, and the items laid at rest on the ground (G1.6). Without it (an earlier version), nothing is
//! an item and every pocket is empty.

use super::SaveError;
use crate::items::{Item, Store};
use creeer_math::codec::{DecodeError, Reader, Writer};
use creeer_math::glam::{DQuat, DVec3};

/// The items of a world, as a save keeps them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedItems {
    /// Item bodies: their entity ids (in order) and items.
    pub bodies: Vec<(u32, Item)>,
    /// Avatars' pockets: their entity ids (in order) and pockets.
    pub pockets: Vec<(u32, Vec<Store>)>,
    /// Loose things picked up, by id, in the order taken.
    pub taken: Vec<u64>,
    /// Items laid at rest on the ground (G1.6): id, item, where it lies and its turn.
    pub laid: Vec<(u64, Item, DVec3, DQuat)>,
}

impl SavedItems {
    /// Whether there is nothing to keep.
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
            && self.pockets.is_empty()
            && self.taken.is_empty()
            && self.laid.is_empty()
    }
}

/// The items as the section holds them.
pub(super) fn items_bytes(items: &SavedItems) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(items.bodies.len() as u64);
    for (id, item) in &items.bodies {
        w.u32(*id);
        w.put(item);
    }
    w.varint(items.pockets.len() as u64);
    for (id, pockets) in &items.pockets {
        w.u32(*id);
        w.put(pockets);
    }
    w.varint(items.taken.len() as u64);
    for id in &items.taken {
        w.u64(*id);
    }
    w.varint(items.laid.len() as u64);
    for (id, item, at, turn) in &items.laid {
        w.u64(*id);
        w.put(item);
        w.put(at);
        w.put(turn);
    }
    w.finish()
}

/// The items from the section.
pub(super) fn items_from(bytes: &[u8]) -> Result<SavedItems, SaveError> {
    let mut r = Reader::new(bytes);
    let bodies = (0..r.count(5)?)
        .map(|_| Ok((r.u32()?, r.get()?)))
        .collect::<Result<Vec<_>, DecodeError>>()?;
    let pockets = (0..r.count(5)?)
        .map(|_| Ok((r.u32()?, r.get()?)))
        .collect::<Result<Vec<_>, DecodeError>>()?;
    let taken = (0..r.count(8)?)
        .map(|_| r.u64())
        .collect::<Result<Vec<_>, _>>()?;
    let laid = (0..r.count(8)?)
        .map(|_| Ok((r.u64()?, r.get()?, r.get()?, r.get()?)))
        .collect::<Result<Vec<_>, DecodeError>>()?;
    r.end()?;
    let ordered = |ids: &mut dyn Iterator<Item = u32>| {
        let ids: Vec<u32> = ids.collect();
        ids.windows(2).all(|w| w[0] < w[1])
    };
    if !ordered(&mut bodies.iter().map(|b| b.0)) || !ordered(&mut pockets.iter().map(|p| p.0)) {
        return Err(SaveError::Damaged(
            "items not in order of their entities".into(),
        ));
    }
    Ok(SavedItems {
        bodies,
        pockets,
        taken,
        laid,
    })
}

#[cfg(test)]
#[path = "items_tests.rs"]
mod tests;
