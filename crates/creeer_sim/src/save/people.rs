//! The `PEOP` section (version 8, G1.2d): players' characters as people, by their avatars'
//! entity ids: their health, and a fallen one's ragdoll (its segments' entity ids, the
//! pelvis the avatar itself first; how long it has lain still, how long dead).

use super::SaveError;
use creeer_math::codec::{DecodeError, Reader, Writer};

/// A person.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedPerson {
    /// Its avatar's entity id.
    pub id: u32,
    /// Its health (a share: 1 whole, 0 dead).
    pub health: f64,
    /// Fallen: its segments' entity ids, the avatar first (empty: standing).
    pub parts: Vec<u32>,
    /// Ticks it has lain still, and been dead.
    pub still: u32,
    pub dead_for: u32,
}

/// The people as the section holds them.
pub(super) fn people_bytes(people: &[SavedPerson]) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(people.len() as u64);
    for p in people {
        w.u32(p.id);
        w.f64(p.health);
        w.put(&p.parts);
        w.u32(p.still);
        w.u32(p.dead_for);
    }
    w.finish()
}

/// The people from the section.
pub(super) fn people_from(bytes: &[u8]) -> Result<Vec<SavedPerson>, SaveError> {
    let mut r = Reader::new(bytes);
    let people = (0..r.count(17)?)
        .map(|_| {
            Ok(SavedPerson {
                id: r.u32()?,
                health: r.finite()?,
                parts: r.get()?,
                still: r.u32()?,
                dead_for: r.u32()?,
            })
        })
        .collect::<Result<Vec<_>, DecodeError>>()?;
    r.end()?;
    Ok(people)
}
