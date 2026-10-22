//! The `ITEM` section (version 10, G1.6a): the bodies that are items (`crate::items`), by their
//! entity ids, and what each avatar's pockets hold. Without it (an earlier version), nothing is
//! an item and every pocket is empty.

use super::SaveError;
use crate::items::{Item, Store};
use creeer_math::codec::{DecodeError, Reader, Writer};

/// The items of a world, as a save keeps them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedItems {
    /// Item bodies: their entity ids (in order) and items.
    pub bodies: Vec<(u32, Item)>,
    /// Avatars' pockets: their entity ids (in order) and pockets.
    pub pockets: Vec<(u32, Vec<Store>)>,
}

impl SavedItems {
    /// Whether there is nothing to keep.
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty() && self.pockets.is_empty()
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
    Ok(SavedItems { bodies, pockets })
}

#[cfg(test)]
#[path = "items_tests.rs"]
mod tests;
