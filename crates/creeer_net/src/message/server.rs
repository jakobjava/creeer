//! What the server sends a client.

pub use super::inventory::{Inventory, ItemView, StoreView};
pub use super::plastic::Plastic;
use super::{read_text, text};
use crate::entity::{EntityId, Kind};
use crate::quant::Quantised;
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};
use creeer_math::glam::DVec3;

/// From the server to a client.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerMessage {
    /// The answer to [`ClientMessage::Hello`].
    Welcome {
        /// The client's id.
        client: u64,
        /// Its avatar.
        avatar: EntityId,
        /// The server's tick now.
        tick: u32,
        /// Ticks per second.
        rate: u32,
        /// The planet the world stands on (its file in the data packs: `planets/home.ron`).
        planet: String,
    },
    /// Refused: why (the protocol differs, the server is full).
    Refused(String),
    /// An entity came into view: what it is.
    Spawn(EntityId, Kind),
    /// An entity left view (or the world).
    Despawn(EntityId),
    /// Terrain edits, as `creeer_terrain::edits` writes a log, the first of them the
    /// `first`-th edit of the world (a client joining gets the whole log); `in_place`: these
    /// replace those edits only, the rest kept (a dig spot's brush grown: G2.9).
    Edits {
        /// The index of the first edit.
        first: u64,
        /// The edits.
        log: Vec<u8>,
        /// Whether these replace those edits only.
        in_place: bool,
    },
    /// A structure deformed for good.
    Plastic(EntityId, Plastic),
    /// A snapshot (`snapshot.rs`).
    Snapshot(Vec<u8>),
    /// Every machine of the world and how it is run (F7.4): sent now and then, to every
    /// client (a machine nobody is near is abstract, beyond every client's view).
    Machines(Vec<MachineView>),
    /// The server's profile, to a client that asked for it (`ClientMessage::Profile`).
    Profile(super::ProfileReport),
    /// The player's own character as only the server knows it (G1.2): its jetpack's gas (kg),
    /// its health (a share of a whole one), whether it is down (its ragdoll: not predicted),
    /// what its pockets hold (kg, G1.6a: on its body, beyond its dry mass and gas),
    /// and what each of the body's parts has taken (head, torso, arms, legs: shares of a whole
    /// health, G1.9), for the HUD and the prediction. Sent with every snapshot round.
    Own {
        gas: f64,
        health: f64,
        hurt: [f64; 4],
        fallen: bool,
        load: f64,
    },
    /// An entity's age (G1.3c, `creeer_sim::ageing`): sent after its spawn, and when its
    /// rate or wear changes.
    Age(EntityId, Age),
    /// A character began a swing of the tool in its hand at network tick `at` (G1.4e): its
    /// blow falls `creeer_sim::strike::TO_BLOW` later.
    Swing(EntityId, u32),
    /// What the player carries (G1.6a, `creeer_sim::items`): their pockets, and the container
    /// they last looked into while it is in reach. Sent when it changes.
    Inventory(Inventory),
    /// Loose things picked up from the ground (G1.6c, `creeer_sim::scatter`), by id: no longer
    /// drawn. A client joining is told all, then each as it is taken.
    Taken(Vec<u64>),
    /// Why the player's last command was refused (G1.6: shown to them, so a refused act is not
    /// silent).
    Refusal(String),
    /// Items laid at rest on the ground (G1.6: drawn and aimed at, no bodies); gone again by
    /// [`ServerMessage::Taken`].
    Laid(Vec<super::Laid>),
    /// What came of something the player did (G1.6: a flake came off, a tool was bound), told to
    /// them as the refusals are.
    Note(String),
    /// What an item body is (G1.6, [`super::ItemLook`]), sent after its spawn.
    Item(EntityId, super::ItemLook),
    /// Standing trees cut into or felled since last told (G1.7; everyone's, as the terrain is).
    Trees(Vec<super::TreeCut>),
    /// The reliable messages after it went with the snapshot of this network tick (sent before
    /// the first of a round's, 2026-10-28): a client drawing the past changes what it draws at
    /// that tick, with that snapshot's bodies, not before (a felled part replaced by its pieces
    /// was drawn by none for 3 ticks).
    Round(u32),
}

/// An entity's age as a client draws it: its exposure (years of weathering) as of network
/// tick `at`, the rate it grows at (years per second) and its wear from use (0–1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Age {
    pub exposure: f64,
    pub rate: f64,
    pub at: u32,
    pub wear: f64,
}

impl Age {
    /// Its exposure at network tick `tick` of `rate` ticks a second (years).
    pub fn exposure_at(&self, tick: f64, rate: u32) -> f64 {
        let seconds = (tick - f64::from(self.at)) / f64::from(rate);
        self.exposure + self.rate * seconds.max(0.0)
    }
}

/// A machine as a panel shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct MachineView {
    pub id: u32,
    pub at: DVec3,
    /// 0: active, 1: abstract, 2: catching up.
    pub mode: u8,
    /// Its upper tank's water (m³) and the hydrogen it made (mol).
    pub water: f64,
    pub hydrogen: f64,
    /// Its time (s).
    pub time: f64,
}

impl Encode for ServerMessage {
    fn encode(&self, w: &mut Writer) {
        match self {
            Self::Welcome {
                client,
                avatar,
                tick,
                rate,
                planet,
            } => {
                w.u8(0);
                w.u64(*client);
                w.put(avatar);
                w.u32(*tick);
                w.u32(*rate);
                text(w, planet);
            }
            Self::Refused(why) => {
                w.u8(1);
                text(w, why);
            }
            Self::Spawn(id, kind) => {
                w.u8(2);
                w.put(id);
                w.put(kind);
            }
            Self::Despawn(id) => {
                w.u8(3);
                w.put(id);
            }
            Self::Edits {
                first,
                log,
                in_place,
            } => {
                w.u8(4);
                w.varint(*first);
                w.bytes_with_length(log);
                w.u8(u8::from(*in_place));
            }
            Self::Plastic(id, plastic) => {
                w.u8(5);
                w.put(id);
                w.varint(plastic.nodes.len() as u64);
                for (node, at) in &plastic.nodes {
                    w.varint(u64::from(*node));
                    at.write(w, Quantised::default());
                }
                match &plastic.members {
                    None => w.u8(0),
                    Some(members) => {
                        w.u8(1);
                        w.varint(members.len() as u64);
                        for [a, b] in members {
                            w.varint(u64::from(*a));
                            w.varint(u64::from(*b));
                        }
                    }
                }
                w.varint(plastic.broken.len() as u64);
                for member in &plastic.broken {
                    w.varint(u64::from(*member));
                }
                w.varint(u64::from(plastic.node_count));
                match &plastic.radii {
                    None => w.u8(0),
                    Some(radii) => {
                        w.u8(1);
                        w.varint(radii.len() as u64);
                        for r in radii {
                            w.varint(crate::entity::tenths(*r));
                        }
                    }
                }
            }
            Self::Machines(list) => {
                w.u8(7);
                w.varint(list.len() as u64);
                for m in list {
                    w.u32(m.id);
                    w.put(&m.at);
                    w.u8(m.mode);
                    w.f64(m.water);
                    w.f64(m.hydrogen);
                    w.f64(m.time);
                }
            }
            Self::Profile(report) => {
                w.u8(8);
                w.put(report);
            }
            Self::Own {
                gas,
                health,
                hurt,
                fallen,
                load,
            } => {
                w.u8(9);
                w.f64(*gas);
                w.f64(*health);
                for h in hurt {
                    w.f64(*h);
                }
                w.put(fallen);
                w.f64(*load);
            }
            Self::Snapshot(bytes) => {
                w.u8(6);
                w.raw(bytes);
            }
            Self::Age(id, age) => {
                w.u8(10);
                w.put(id);
                w.f64(age.exposure);
                w.f64(age.rate);
                w.u32(age.at);
                w.f64(age.wear);
            }
            Self::Swing(id, at) => {
                w.u8(11);
                w.put(id);
                w.u32(*at);
            }
            Self::Inventory(inventory) => {
                w.u8(12);
                w.put(inventory);
            }
            Self::Taken(ids) => {
                w.u8(13);
                w.varint(ids.len() as u64);
                for id in ids {
                    w.u64(*id);
                }
            }
            Self::Refusal(why) => {
                w.u8(14);
                text(w, why);
            }
            Self::Laid(laid) => {
                w.u8(15);
                w.put(laid);
            }
            Self::Note(note) => {
                w.u8(16);
                text(w, note);
            }
            Self::Item(id, look) => {
                w.u8(17);
                w.put(id);
                w.put(look);
            }
            Self::Trees(cuts) => {
                w.u8(18);
                w.put(cuts);
            }
            Self::Round(tick) => {
                w.u8(19);
                w.u32(*tick);
            }
        }
    }
}

impl Decode for ServerMessage {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let index = |r: &mut Reader<'_>| u32::try_from(r.varint()?).map_err(|_| r.error("index"));
        Ok(match r.u8()? {
            0 => Self::Welcome {
                client: r.u64()?,
                avatar: r.get()?,
                tick: r.u32()?,
                rate: r.u32()?,
                planet: read_text(r)?,
            },
            1 => Self::Refused(read_text(r)?),
            2 => Self::Spawn(r.get()?, r.get()?),
            3 => Self::Despawn(r.get()?),
            4 => Self::Edits {
                first: r.varint()?,
                log: r.bytes_with_length()?.to_vec(),
                in_place: r.u8()? == 1,
            },
            5 => {
                let id = r.get()?;
                let nodes = (0..r.count(4)?)
                    .map(|_| Ok((index(r)?, Quantised::read(r, Quantised::default())?)))
                    .collect::<Result<_, DecodeError>>()?;
                let members = match r.u8()? {
                    0 => None,
                    1 => Some(
                        (0..r.count(2)?)
                            .map(|_| Ok([index(r)?, index(r)?]))
                            .collect::<Result<_, DecodeError>>()?,
                    ),
                    _ => return Err(r.error("not a flag")),
                };
                let broken = (0..r.count(1)?)
                    .map(|_| index(r))
                    .collect::<Result<_, DecodeError>>()?;
                let node_count =
                    u32::try_from(r.varint()?).map_err(|_| r.error("too many nodes"))?;
                let radii = match r.u8()? {
                    0 => None,
                    1 => Some(
                        (0..r.count(1)?)
                            .map(|_| Ok(r.varint()? as f64 * 1e-4))
                            .collect::<Result<_, DecodeError>>()?,
                    ),
                    _ => return Err(r.error("not a flag")),
                };
                Self::Plastic(
                    id,
                    Plastic {
                        nodes,
                        members,
                        broken,
                        node_count,
                        radii,
                    },
                )
            }
            6 => Self::Snapshot(r.raw(r.remaining())?.to_vec()),
            7 => {
                let mut list = Vec::new();
                for _ in 0..r.count(41)? {
                    list.push(MachineView {
                        id: r.u32()?,
                        at: r.get()?,
                        mode: r.u8()?,
                        water: r.f64()?,
                        hydrogen: r.f64()?,
                        time: r.f64()?,
                    });
                }
                Self::Machines(list)
            }
            8 => Self::Profile(r.get()?),
            9 => Self::Own {
                gas: r.finite()?,
                health: r.finite()?,
                hurt: [r.finite()?, r.finite()?, r.finite()?, r.finite()?],
                fallen: r.get()?,
                load: r.finite()?,
            },
            10 => Self::Age(
                r.get()?,
                Age {
                    exposure: r.finite()?,
                    rate: r.finite()?,
                    at: r.u32()?,
                    wear: r.finite()?,
                },
            ),
            11 => Self::Swing(r.get()?, r.u32()?),
            12 => Self::Inventory(r.get()?),
            13 => Self::Taken(
                (0..r.count(8)?)
                    .map(|_| r.u64())
                    .collect::<Result<_, _>>()?,
            ),
            14 => Self::Refusal(read_text(r)?),
            15 => Self::Laid(r.get()?),
            16 => Self::Note(read_text(r)?),
            17 => Self::Item(r.get()?, r.get()?),
            18 => Self::Trees(r.get()?),
            19 => Self::Round(r.u32()?),
            _ => return Err(r.error("unknown server message")),
        })
    }
}
