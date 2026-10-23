//! The `BILT` section (version 13, G2.3): what each built structure is made of. By entity: the
//! items its parts and fasteners were (what salvage gives back, G2.6), and each element's wood
//! (an engineering material's id; none for a joint), from which later joints' laws are worked
//! out. The structures themselves are bodies of the world (`WRLD`). Without it (an earlier
//! version), nothing was built.

use super::SaveError;
use crate::items::Item;
use creeer_math::codec::{Reader, Writer};

/// One built structure's record: its entity, its items, its elements' woods.
pub type SavedBuilt = (u32, Vec<Item>, Vec<Option<String>>);

/// The records as the section holds them.
pub(super) fn built_bytes(built: &[SavedBuilt]) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(built.len() as u64);
    for (id, items, woods) in built {
        w.u32(*id);
        w.put(items);
        w.put(woods);
    }
    w.finish()
}

/// The records from the section.
pub(super) fn built_from(bytes: &[u8]) -> Result<Vec<SavedBuilt>, SaveError> {
    let mut r = Reader::new(bytes);
    let mut built = Vec::new();
    // Each at least 6 bytes: an id and two counts.
    for _ in 0..r.count(6)? {
        built.push((r.u32()?, r.get()?, r.get()?));
    }
    r.end()?;
    Ok(built)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::{Form, Piece};

    #[test]
    fn built_records_round_trip() {
        let pole = Item::Piece(
            Piece::solid(
                "spruce_green",
                800.0,
                Form::Rod {
                    radius: 0.025,
                    length: 1.6,
                },
            ),
            Vec::new(),
        );
        let built = vec![(
            7,
            vec![pole.clone(), pole],
            vec![Some("spruce_green".into()), None],
        )];
        assert_eq!(built_from(&built_bytes(&built)), Ok(built));
    }
}
