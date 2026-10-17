//! Saves: a world on a planet as a file, and the files of earlier versions read as they are
//! now (migration).
//!
//! **Version 4** (F6): as version 3, a structure's solver keeping the last step's external
//! loads and torques (what its next Newmark step carries on, for the energy guard).
//!
//! **Version 3** (F6): as version 2, a member of a structure keeping its thermal elongation
//! (after its plastic strain).
//!
//! **Version 2** (F5): the signature `CREEERSV`, the version (u32, little-endian), then
//! sections, each a four-letter tag and its bytes with their length:
//!
//! - `META`: the planet's data file (as `creeer_terrain::data_path` takes it), the tick, and
//!   the next entity id to hand out;
//! - `EDIT`: the terrain's edits, as `creeer_terrain::edits` writes a log;
//! - `WRLD`: the physics world (`creeer_physics::world::save`: every body, its history, the
//!   friction anchors; the ground is the terrain's);
//! - `ENTS`: for each body of the world in its order, its entity id, and whose avatar it is
//!   if it is one (the player's name: they get it back when they come again).
//!
//! A section with a tag this version does not know is skipped (a later version may add
//! one that an older game can do without); one it needs and does not find is an error.
//!
//! **Version 1** (F3): the planet sandbox's edit log alone (`CREEDITS`): read as a world on
//! the reference planet with those edits and nothing standing on it.

use creeer_math::codec::{DecodeError, Reader, Writer};
use creeer_physics::world::World;
use creeer_terrain::edits::Edits;

/// The version this game writes.
pub const VERSION: u32 = 4;
/// The signature of a save.
const MAGIC: &[u8; 8] = b"CREEERSV";
/// The signature of a version 1 save (an edit log).
const V1_MAGIC: &[u8; 8] = b"CREEDITS";
/// The planet a version 1 save is of (the F3 sandbox had only it).
pub const V1_PLANET: &str = "planets/home.ron";

/// What a save holds.
#[derive(Clone, Debug)]
pub struct Save {
    /// The planet's data file.
    pub planet: String,
    /// The server's tick.
    pub tick: u64,
    /// The next entity id to hand out.
    pub next_id: u32,
    /// The terrain's edits.
    pub edits: Edits,
    /// The physics world, without its ground.
    pub world: World,
    /// For each body of `world`: its entity id, and the player whose avatar it is.
    pub entities: Vec<(u32, Option<String>)>,
}

/// Why a file could not be read as a save.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// It is no save.
    NotASave,
    /// It is a save of a later version than this game reads.
    Newer(u32),
    /// It is damaged (where and how).
    Damaged(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotASave => f.write_str("not a save"),
            Self::Newer(v) => write!(
                f,
                "saved by a later game (version {v}; this one reads up to {VERSION})"
            ),
            Self::Damaged(why) => write!(f, "damaged save: {why}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<DecodeError> for SaveError {
    fn from(e: DecodeError) -> Self {
        Self::Damaged(e.to_string())
    }
}

fn section(w: &mut Writer, tag: &[u8; 4], bytes: &[u8]) {
    w.raw(tag);
    w.bytes_with_length(bytes);
}

impl Save {
    /// The save as bytes (version [`VERSION`]).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.raw(MAGIC);
        w.u32(VERSION);
        w.varint(4);
        let mut meta = Writer::new();
        meta.put(&self.planet);
        meta.u64(self.tick);
        meta.u32(self.next_id);
        section(&mut w, b"META", meta.bytes());
        section(&mut w, b"EDIT", &self.edits.to_bytes());
        section(&mut w, b"WRLD", &self.world.to_bytes());
        let mut ents = Writer::new();
        ents.varint(self.entities.len() as u64);
        for (id, player) in &self.entities {
            ents.u32(*id);
            ents.put(player);
        }
        section(&mut w, b"ENTS", ents.bytes());
        w.finish()
    }

    /// A save from bytes of any version this game reads.
    ///
    /// # Errors
    /// If the bytes are no save, a later version's, or damaged.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveError> {
        match bytes.get(..8) {
            Some(m) if m == V1_MAGIC => from_v1(bytes),
            Some(m) if m == MAGIC => from_sections(bytes),
            _ => Err(SaveError::NotASave),
        }
    }
}

fn from_v1(bytes: &[u8]) -> Result<Save, SaveError> {
    let edits = Edits::from_bytes(bytes).map_err(|e| SaveError::Damaged(e.0))?;
    Ok(Save {
        planet: V1_PLANET.to_owned(),
        tick: 0,
        next_id: 1,
        edits,
        world: World::new(1.0 / 60.0),
        entities: Vec::new(),
    })
}

fn from_sections(bytes: &[u8]) -> Result<Save, SaveError> {
    let mut r = Reader::new(bytes);
    r.raw(8)?;
    let version = r.u32()?;
    if version > VERSION {
        return Err(SaveError::Newer(version));
    }
    let (mut meta, mut edits, mut world, mut ents) = (None, None, None, None);
    for _ in 0..r.count(5)? {
        let tag: [u8; 4] = r.raw(4)?.try_into().expect("four bytes");
        let body = r.bytes_with_length()?;
        match &tag {
            b"META" => meta = Some(body),
            b"EDIT" => edits = Some(body),
            b"WRLD" => world = Some(body),
            b"ENTS" => ents = Some(body),
            _ => {} // a later version's: not needed here
        }
    }
    r.end()?;
    let missing = |what: &str| SaveError::Damaged(format!("no {what} section"));
    let mut m = Reader::new(meta.ok_or_else(|| missing("META"))?);
    let planet: String = m.get()?;
    let (tick, next_id) = (m.u64()?, m.u32()?);
    m.end()?;
    let edits = Edits::from_bytes(edits.ok_or_else(|| missing("EDIT"))?)
        .map_err(|e| SaveError::Damaged(e.0))?;
    let world = World::from_bytes_of(world.ok_or_else(|| missing("WRLD"))?, version)?;
    let mut e = Reader::new(ents.ok_or_else(|| missing("ENTS"))?);
    let entities = (0..e.count(5)?)
        .map(|_| {
            let id = e.u32()?;
            Ok((id, e.get::<Option<String>>()?))
        })
        .collect::<Result<Vec<_>, DecodeError>>()?;
    e.end()?;
    if entities.len() != world.bodies.len() {
        return Err(SaveError::Damaged("entities and bodies disagree".into()));
    }
    let mut ids: Vec<u32> = entities.iter().map(|e| e.0).collect();
    ids.sort_unstable();
    if ids.windows(2).any(|w| w[0] == w[1]) || ids.last().is_some_and(|&last| last >= next_id) {
        return Err(SaveError::Damaged(
            "entity ids repeat or run past the next".into(),
        ));
    }
    Ok(Save {
        planet,
        tick,
        next_id,
        edits,
        world,
        entities,
    })
}

#[cfg(test)]
mod tests;
