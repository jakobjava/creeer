//! The `AGES` section (version 9, G1.3c): each entity's age (`crate::ageing`), by its entity
//! id: its exposure as of a tick, the rate it weathers at since, and its wear. Without it (an
//! earlier version), everything is new.

use super::SaveError;
use crate::ageing::Weathering;
use creeer_math::codec::{Reader, Writer};

/// The ages as the section holds them.
pub(super) fn ages_bytes(ages: &[(u32, Weathering)]) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(ages.len() as u64);
    for (id, age) in ages {
        w.u32(*id);
        age.write(&mut w);
    }
    w.finish()
}

/// The ages from the section.
pub(super) fn ages_from(bytes: &[u8]) -> Result<Vec<(u32, Weathering)>, SaveError> {
    let mut r = Reader::new(bytes);
    let mut ages = Vec::new();
    // Each at least 36 bytes: an id and four numbers.
    for _ in 0..r.count(36)? {
        let id = r.u32()?;
        ages.push((id, Weathering::read(&mut r)?));
    }
    r.end()?;
    if ages.windows(2).any(|w| w[0].0 >= w[1].0) {
        return Err(SaveError::Damaged(
            "ages not in order of their entities".into(),
        ));
    }
    Ok(ages)
}
