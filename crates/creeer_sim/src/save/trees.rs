//! The `TREE` section (version 11, G1.7): what was done to the trees (`crate::trees`): cuts into
//! standing trees, the trees felled and the height they were cut at, the bodies that are felled
//! trees' parts (by their entity ids) and which of them are still on their hinge, how many of
//! the world's fixed shapes are trunks, and the cuts and limbing in progress on parts. Without it
//! (an earlier version), every tree stands whole.

use super::SaveError;
use crate::trees::{Cut, Part};
use creeer_math::codec::{DecodeError, Reader, Writer};

/// The trees of a world, as a save keeps them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SavedTrees {
    pub cuts: Vec<(u64, Cut)>,
    pub felled: Vec<(u64, f64)>,
    pub parts: Vec<(u32, Part)>,
    pub hinged: Vec<u32>,
    pub trunks: u64,
    pub bucks: Vec<(u32, Cut)>,
    /// A part's node being limbed, and the wood taken out of its branches so far (m³).
    pub limbing: Vec<(u32, u64, f64)>,
    /// The cells whose trees are made (their keys: the trees are made again from them), those
    /// waiting, and how many of the terrain's edits they have seen.
    pub cells: Vec<u64>,
    pub pending: Vec<u64>,
    pub edits: u64,
}

impl SavedTrees {
    /// Whether there is nothing to keep.
    pub fn is_empty(&self) -> bool {
        self.cuts.is_empty()
            && self.felled.is_empty()
            && self.parts.is_empty()
            && self.trunks == 0
            && self.cells.is_empty()
            && self.pending.is_empty()
    }
}

fn put_cut(w: &mut Writer, cut: &Cut) {
    w.f64(cut.z);
    w.varint(cut.notches.len() as u64);
    for (side, volume) in &cut.notches {
        w.f64(*side);
        w.f64(*volume);
    }
}

fn get_cut(r: &mut Reader<'_>) -> Result<Cut, DecodeError> {
    let z = r.finite()?;
    let notches = (0..r.count(16)?)
        .map(|_| Ok((r.finite()?, r.finite()?)))
        .collect::<Result<_, DecodeError>>()?;
    Ok(Cut { z, notches })
}

/// The trees as the section holds them.
pub(super) fn trees_bytes(t: &SavedTrees) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(t.cuts.len() as u64);
    for (id, cut) in &t.cuts {
        w.u64(*id);
        put_cut(&mut w, cut);
    }
    w.varint(t.felled.len() as u64);
    for (id, z) in &t.felled {
        w.u64(*id);
        w.f64(*z);
    }
    w.varint(t.parts.len() as u64);
    for (id, p) in &t.parts {
        w.u32(*id);
        w.u16(p.species);
        w.u64(p.seed);
        for v in [p.dbh, p.from, p.to] {
            w.f64(v);
        }
        w.bytes_with_length(&p.cut);
    }
    w.varint(t.hinged.len() as u64);
    for id in &t.hinged {
        w.u32(*id);
    }
    w.u64(t.trunks);
    w.varint(t.bucks.len() as u64);
    for (id, cut) in &t.bucks {
        w.u32(*id);
        put_cut(&mut w, cut);
    }
    w.varint(t.limbing.len() as u64);
    for (id, node, volume) in &t.limbing {
        w.u32(*id);
        w.u64(*node);
        w.f64(*volume);
    }
    for keys in [&t.cells, &t.pending] {
        w.varint(keys.len() as u64);
        for k in keys {
            w.u64(*k);
        }
    }
    w.u64(t.edits);
    w.finish()
}

/// The trees from the section's bytes.
pub(super) fn trees_from(bytes: &[u8]) -> Result<SavedTrees, SaveError> {
    let mut r = Reader::new(bytes);
    let cuts = (0..r.count(9)?)
        .map(|_| Ok((r.u64()?, get_cut(&mut r)?)))
        .collect::<Result<_, DecodeError>>()?;
    let felled = (0..r.count(16)?)
        .map(|_| Ok((r.u64()?, r.finite()?)))
        .collect::<Result<_, DecodeError>>()?;
    let parts = (0..r.count(38)?)
        .map(|_| {
            let id = r.u32()?;
            Ok((
                id,
                Part {
                    species: r.u16()?,
                    seed: r.u64()?,
                    dbh: r.finite()?,
                    from: r.finite()?,
                    to: r.finite()?,
                    cut: r.bytes_with_length()?.to_vec(),
                },
            ))
        })
        .collect::<Result<_, DecodeError>>()?;
    let hinged = (0..r.count(4)?)
        .map(|_| r.u32())
        .collect::<Result<_, _>>()?;
    let trunks = r.u64()?;
    let bucks = (0..r.count(13)?)
        .map(|_| Ok((r.u32()?, get_cut(&mut r)?)))
        .collect::<Result<_, DecodeError>>()?;
    let limbing = (0..r.count(20)?)
        .map(|_| Ok((r.u32()?, r.u64()?, r.finite()?)))
        .collect::<Result<_, DecodeError>>()?;
    let cells = (0..r.count(8)?)
        .map(|_| r.u64())
        .collect::<Result<_, _>>()?;
    let pending = (0..r.count(8)?)
        .map(|_| r.u64())
        .collect::<Result<_, _>>()?;
    let edits = r.u64()?;
    r.end()?;
    Ok(SavedTrees {
        cuts,
        felled,
        parts,
        hinged,
        trunks,
        bucks,
        limbing,
        cells,
        pending,
        edits,
    })
}

#[cfg(test)]
#[path = "trees_tests.rs"]
mod tests;
