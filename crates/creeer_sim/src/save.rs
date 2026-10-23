//! Saves: a world on a planet as a file, and the files of earlier versions read as they are
//! now (migration).
//!
//! **Version 14** (G2.3): as version 13, with an optional section `BILT`: what each built
//! structure is made of (`save/built.rs`).
//!
//! **Version 13** (G2.0): as version 12, the physics world's friction anchors keeping the
//! inset of each bump of the ground they belong to (`WRLD`; ADR-0024's "G2.0: rest").
//!
//! **Version 12** (G1.5): as version 11, with an optional section `PITS`: what was dug and not
//! yet cut from the terrain (`save/pits.rs`).
//!
//! **Version 11** (G1.7): as version 10, with an optional section `TREE`: what was done to the
//! trees: cuts, the felled, felled trees' parts as bodies (`crate::trees`; `save/trees.rs`).
//!
//! **Version 10** (G1.6a): as version 9, with an optional section `ITEM`: the bodies that are
//! items, and avatars' pockets (`crate::items`; `save/items.rs`).
//!
//! **Version 9** (G1.3c): as version 8, with an optional section `AGES`: each entity's age (its
//! weathering and wear, `crate::ageing`; `save/ages.rs`).
//!
//! **Version 8** (G1): as version 7, the physics world keeping its joints (`WRLD`; ADR-0022), and
//! an optional section `PEOP`: players' characters as people, their health and a fallen one's
//! ragdoll (G1.2d; ADR-0023).
//!
//! **Version 7** (F7): as version 6, with an optional section `MACH`: the world's machines,
//! each as its abstract self keeps it (`save/machines.rs`).
//!
//! **Version 6** (F7): as version 5, with an optional section `PARK`: the islands of bodies
//! parked far from every player (`crate::lod`), as a physics world of their bodies in island
//! order, then for each island its number of bodies, and for each body its entity id, whose
//! avatar it is, and the water moving with its floating shapes.
//!
//! **Version 5** (F6): as version 4, the world keeping the elastic energy each contact held
//! at the end of the last step (what the next step's energy guard holds its contacts, found anew,
//! to: `creeer_physics::world::left`).
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
//! - `META`: the planet's file in the data packs (`planets/home.ron`), the tick, and
//!   the next entity id to hand out;
//! - `EDIT`: the terrain's edits, as `creeer_terrain::edits` writes a log;
//! - `WRLD`: the physics world (`creeer_physics::world::save`: every body, its history, the
//!   friction anchors; the ground is the terrain's);
//! - `ENTS`: for each body of the world in its order, its entity id, and whose avatar it is
//!   if it is one (the player's name: they get it back when they come again).
//! - `SURR` (F6.6, optional): the water moving with each floating shape, in the order the
//!   planet's surroundings meet them (`creeer_sim::surroundings`), so a floating body goes
//!   on as it would have; without it the water is taken as still.
//!
//! A section with a tag this version does not know is skipped (a later version may add
//! one that an older game can do without); one it needs and does not find is an error.
//!
//! **Version 1** (F3): the planet sandbox's edit log alone (`CREEDITS`): read as a world on
//! the reference planet with those edits and nothing standing on it.

use creeer_math::codec::{DecodeError, Reader, Writer};
use creeer_physics::world::{Body, World};
use creeer_terrain::edits::Edits;

mod ages;
mod items;
mod machines;
mod park;
mod people;
mod built;
mod pits;
pub use built::SavedBuilt;
mod trees;
pub use items::SavedItems;
pub use machines::SavedMachine;
use park::{park_bytes, park_from};
pub use people::SavedPerson;
pub use trees::SavedTrees;

/// The version this game writes.
pub const VERSION: u32 = 14;
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
    /// The water moving with each floating shape (`creeer_sim::surroundings`; none: still).
    pub water: Vec<f64>,
    /// The islands parked far from every player (`crate::lod`).
    pub parked: Vec<Vec<ParkedBody>>,
    /// The world's machines (`crate::abstraction`).
    pub machines: Vec<SavedMachine>,
    /// Players' characters as people (`crate::character`): health, a fallen one's ragdoll.
    pub people: Vec<SavedPerson>,
    /// Each entity's age (`crate::ageing`), in order of their ids.
    pub ages: Vec<(u32, crate::ageing::Weathering)>,
    /// The items (`crate::items`): bodies that are items, and avatars' pockets.
    pub items: SavedItems,
    /// What was done to the trees (`crate::trees`).
    pub trees: SavedTrees,
    /// What was dug and not yet cut from the terrain, by spot: where (m) and how much (m³).
    pub pits: Vec<(creeer_math::glam::DVec3, f64)>,
    /// What each built structure is made of (version 13, G2.3).
    pub built: Vec<SavedBuilt>,
}

/// A body parked far from every player, with what the world keeps of it.
#[derive(Clone, Debug)]
pub struct ParkedBody {
    pub id: u32,
    pub player: Option<String>,
    pub body: Body,
    pub water: Vec<f64>,
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
        let sections = 4
            + u64::from(!self.water.is_empty())
            + u64::from(!self.parked.is_empty())
            + u64::from(!self.machines.is_empty())
            + u64::from(!self.people.is_empty())
            + u64::from(!self.ages.is_empty())
            + u64::from(!self.items.is_empty())
            + u64::from(!self.trees.is_empty())
            + u64::from(!self.pits.is_empty())
            + u64::from(!self.built.is_empty());
        w.varint(sections);
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
        if !self.water.is_empty() {
            let mut surr = Writer::new();
            surr.varint(self.water.len() as u64);
            for &m in &self.water {
                surr.f64(m);
            }
            section(&mut w, b"SURR", surr.bytes());
        }
        if !self.parked.is_empty() {
            section(&mut w, b"PARK", &park_bytes(&self.parked, self.world.dt()));
        }
        if !self.machines.is_empty() {
            section(&mut w, b"MACH", &machines::machine_bytes(&self.machines));
        }
        if !self.people.is_empty() {
            section(&mut w, b"PEOP", &people::people_bytes(&self.people));
        }
        if !self.ages.is_empty() {
            section(&mut w, b"AGES", &ages::ages_bytes(&self.ages));
        }
        if !self.items.is_empty() {
            section(&mut w, b"ITEM", &items::items_bytes(&self.items));
        }
        if !self.trees.is_empty() {
            section(&mut w, b"TREE", &trees::trees_bytes(&self.trees));
        }
        if !self.pits.is_empty() {
            section(&mut w, b"PITS", &pits::pits_bytes(&self.pits));
        }
        if !self.built.is_empty() {
            section(&mut w, b"BILT", &built::built_bytes(&self.built));
        }
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
        water: Vec::new(),
        parked: Vec::new(),
        machines: Vec::new(),
        people: Vec::new(),
        ages: Vec::new(),
        items: SavedItems::default(),
        trees: SavedTrees::default(),
        pits: Vec::new(),
        built: Vec::new(),
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
    let (mut surr, mut park, mut mach, mut peop) = (None, None, None, None);
    let (mut ages, mut item, mut tree, mut pit) = (None, None, None, None);
    let mut bilt = None;
    for _ in 0..r.count(5)? {
        let tag: [u8; 4] = r.raw(4)?.try_into().expect("four bytes");
        let body = r.bytes_with_length()?;
        match &tag {
            b"META" => meta = Some(body),
            b"EDIT" => edits = Some(body),
            b"WRLD" => world = Some(body),
            b"ENTS" => ents = Some(body),
            b"SURR" => surr = Some(body),
            b"PARK" => park = Some(body),
            b"MACH" => mach = Some(body),
            b"PEOP" => peop = Some(body),
            b"AGES" => ages = Some(body),
            b"ITEM" => item = Some(body),
            b"TREE" => tree = Some(body),
            b"PITS" => pit = Some(body),
            b"BILT" => bilt = Some(body),
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
    let parked = match park {
        Some(bytes) => park_from(bytes, version)?,
        None => Vec::new(),
    };
    let parked_ids = parked.iter().flatten().map(|p| p.id);
    let mut ids: Vec<u32> = entities.iter().map(|e| e.0).chain(parked_ids).collect();
    ids.sort_unstable();
    if ids.windows(2).any(|w| w[0] == w[1]) || ids.last().is_some_and(|&last| last >= next_id) {
        return Err(SaveError::Damaged(
            "entity ids repeat or run past the next".into(),
        ));
    }
    let mut water = Vec::new();
    if let Some(body) = surr {
        let mut s = Reader::new(body);
        for _ in 0..s.count(8)? {
            water.push(s.f64()?);
        }
        s.end()?;
    }
    let machines = match mach {
        Some(bytes) => machines::machines_from(bytes)?,
        None => Vec::new(),
    };
    let people = match peop {
        Some(bytes) => people::people_from(bytes)?,
        None => Vec::new(),
    };
    let ages = match ages {
        Some(bytes) => ages::ages_from(bytes)?,
        None => Vec::new(),
    };
    let items = item.map_or_else(|| Ok(SavedItems::default()), items::items_from)?;
    let trees = tree.map_or_else(|| Ok(SavedTrees::default()), trees::trees_from)?;
    let pits = pit.map_or_else(|| Ok(Vec::new()), pits::pits_from)?;
    let built = bilt.map_or_else(|| Ok(Vec::new()), built::built_from)?;
    // A fallen person's segments are bodies of the world (a ragdoll is never parked).
    let live = |id: &u32| entities.iter().any(|e| e.0 == *id);
    if !people.iter().all(|p| p.parts.iter().all(live)) {
        return Err(SaveError::Damaged(
            "a fallen person's segments are not in the world".into(),
        ));
    }
    Ok(Save {
        planet,
        tick,
        next_id,
        edits,
        world,
        entities,
        water,
        parked,
        machines,
        people,
        ages,
        items,
        trees,
        pits,
        built,
    })
}

#[cfg(test)]
mod ages_tests;
#[cfg(test)]
mod joint_tests;
#[cfg(test)]
mod machine_tests;
#[cfg(test)]
mod tests;
