//! What a client asks the world for (reliable), and how it is written.

use super::NONE;
use crate::entity::EntityId;
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};
use creeer_math::glam::DVec3;

/// What a client asks the world for (reliable).
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Drop a 1 m crate at a point, at rest.
    Crate(DVec3),
    /// Drop a ball at a point, at rest.
    Ball(DVec3),
    /// Change the terrain: one edit as `creeer_terrain::edits` writes it.
    Edit(Vec<u8>),
    /// Remove every body but the avatars.
    Clear,
    /// Drop a steel space frame (2 × 2 × 2 cells of 1 m, welded tubes) at a point, at rest.
    Frame(DVec3),
    /// Build a powerhouse (F6.8's machine) at a point.
    Powerhouse(DVec3),
    /// The player's chosen skin tone and hair colour (G1.3e: indices into the palette's
    /// lists; the server keeps their colour and outfit).
    Look { skin: u8, hair: u8 },
    /// The tool the player holds in the right hand (G1.4c: an index into the tools' table,
    /// `creeer_sim::tools`; none, empty-handed).
    Wield { tool: Option<u8> },
    /// Strike what the player aims at with the tool in hand (G1.4e), the swing begun at the
    /// player's own tick `started` (fractional, as snapshots are numbered: its avatar's, ahead).
    Strike { target: Target, started: f64 },
    /// Take what the player aims at in both hands (G1.4f), the tool in hand put away.
    Grab(Target),
    /// Let go of what the hands hold.
    Drop,
    /// Throw what the hands hold along a direction (the world's frame).
    Throw(DVec3),
    /// Drop a box at a point, at rest (G1.4f: 0.4 m of wood, 15 kg, to be carried).
    Box(DVec3),
    /// Drop one of the creative kit's items at a point (G1.6a: an index into `kit.ron`).
    Kit { item: u8, at: DVec3 },
    /// Put away into the first pocket it fits (G1.6a): what the hands hold (none), or an item
    /// the player reaches for.
    Stow(Option<Target>),
    /// Take item `item` of pocket `pocket` out, into the hands.
    TakeOut { pocket: u8, item: u16 },
    /// Put what the hands hold into the container the player reaches for.
    PutIn(Target),
    /// Take item `item` out of the container the player reaches for, into the hands.
    TakeFrom { container: Target, item: u16 },
    /// Look into the container the player reaches for (its contents come with the inventory).
    Open(Target),
    /// Pick up loose thing `loose` from the ground (G1.6c: a stone or a branch the scatter put
    /// there, by its id): into the hands, or into the first pocket it fits.
    Pick { loose: u64, pocket: bool },
    /// Strip bark from the branch in the hands (G1.6d): the strip into a pocket.
    Strip,
    /// Bind the head the player reaches for to the haft in the hands, with a strip from a pocket.
    Bind(Target),
    /// Strike loose thing `loose` on the ground (G1.6: a stone lying there, by its id) with what
    /// the hands hold, the swing begun at the player's own tick `started`.
    StrikeLoose { loose: u64, started: f64 },
    /// Bind the thing lying on the ground at `loose` (by its id) to the haft in the hands.
    BindLying(u64),
    /// Strike standing tree `tree` (G1.7: by its id) at `height` m up its stem with what the
    /// hands hold, the swing begun at the player's own tick `started`.
    StrikeTree {
        tree: u64,
        height: f64,
        started: f64,
    },
    /// Fastens the two parts touching nearest the target with one of a fastener (G2.3: its
    /// name in the catalogue, `data/fasteners.ron`).
    Fasten { target: Target, fastener: String },
    /// Takes the fasteners out of the joint nearest the target on a built structure (G2.6).
    Unfasten(Target),
}

/// What a player aims at, as they saw it (G1.4d): a point on a body (none: the ground), in the
/// world's frame, and the server tick it was drawn at (fractional: as snapshots are numbered).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub body: Option<EntityId>,
    pub at: DVec3,
    pub seen: f64,
}

impl Encode for Target {
    fn encode(&self, w: &mut Writer) {
        match self.body {
            Some(id) => {
                w.u8(1);
                w.put(&id);
            }
            None => w.u8(0),
        }
        w.put(&self.at);
        w.f64(self.seen);
    }
}

impl Decode for Target {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let body = match r.u8()? {
            0 => None,
            1 => Some(r.get()?),
            _ => return Err(r.error("not a flag")),
        };
        Ok(Self {
            body,
            at: r.get()?,
            seen: r.f64()?,
        })
    }
}

impl Encode for Command {
    fn encode(&self, w: &mut Writer) {
        match self {
            Self::Crate(at) => {
                w.u8(0);
                w.put(at);
            }
            Self::Ball(at) => {
                w.u8(1);
                w.put(at);
            }
            Self::Edit(bytes) => {
                w.u8(2);
                w.bytes_with_length(bytes);
            }
            Self::Clear => w.u8(3),
            Self::Powerhouse(at) => {
                w.u8(5);
                w.put(at);
            }
            Self::Frame(at) => {
                w.u8(4);
                w.put(at);
            }
            Self::Look { skin, hair } => {
                w.u8(6);
                w.u8(*skin);
                w.u8(*hair);
            }
            Self::Wield { tool } => {
                w.u8(7);
                w.u8(tool.unwrap_or(NONE));
            }
            Self::Strike { target, started } => {
                w.u8(8);
                w.put(target);
                w.f64(*started);
            }
            Self::Grab(target) => {
                w.u8(9);
                w.put(target);
            }
            Self::Drop => w.u8(10),
            Self::Throw(direction) => {
                w.u8(11);
                w.put(direction);
            }
            Self::Box(at) => {
                w.u8(12);
                w.put(at);
            }
            Self::Kit { item, at } => {
                w.u8(13);
                w.u8(*item);
                w.put(at);
            }
            Self::Stow(target) => {
                w.u8(14);
                w.put(target);
            }
            Self::TakeOut { pocket, item } => {
                w.u8(15);
                w.u8(*pocket);
                w.varint(u64::from(*item));
            }
            Self::PutIn(target) => {
                w.u8(16);
                w.put(target);
            }
            Self::TakeFrom { container, item } => {
                w.u8(17);
                w.put(container);
                w.varint(u64::from(*item));
            }
            Self::Open(target) => {
                w.u8(18);
                w.put(target);
            }
            Self::Pick { loose, pocket } => {
                w.u8(19);
                w.u64(*loose);
                w.put(pocket);
            }
            Self::Strip => w.u8(20),
            Self::Bind(target) => {
                w.u8(21);
                w.put(target);
            }
            Self::StrikeLoose { loose, started } => {
                w.u8(22);
                w.u64(*loose);
                w.f64(*started);
            }
            Self::BindLying(id) => {
                w.u8(23);
                w.u64(*id);
            }
            Self::StrikeTree {
                tree,
                height,
                started,
            } => {
                w.u8(24);
                w.u64(*tree);
                w.f64(*height);
                w.f64(*started);
            }
            Self::Fasten { target, fastener } => {
                w.u8(25);
                w.put(target);
                w.put(fastener);
            }
            Self::Unfasten(target) => {
                w.u8(26);
                w.put(target);
            }
        }
    }
}

impl Decode for Command {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            0 => Command::Crate(r.get()?),
            1 => Command::Ball(r.get()?),
            2 => Command::Edit(r.bytes_with_length()?.to_vec()),
            3 => Command::Clear,
            4 => Command::Frame(r.get()?),
            5 => Command::Powerhouse(r.get()?),
            6 => Command::Look {
                skin: r.u8()?,
                hair: r.u8()?,
            },
            7 => Command::Wield {
                tool: Some(r.u8()?).filter(|&t| t != NONE),
            },
            8 => Command::Strike {
                target: r.get()?,
                started: r.f64()?,
            },
            9 => Command::Grab(r.get()?),
            10 => Command::Drop,
            11 => Command::Throw(r.get()?),
            12 => Command::Box(r.get()?),
            13 => Command::Kit {
                item: r.u8()?,
                at: r.get()?,
            },
            14 => Command::Stow(r.get()?),
            15 => Command::TakeOut {
                pocket: r.u8()?,
                item: index(r)?,
            },
            16 => Command::PutIn(r.get()?),
            17 => Command::TakeFrom {
                container: r.get()?,
                item: index(r)?,
            },
            18 => Command::Open(r.get()?),
            19 => Command::Pick {
                loose: r.u64()?,
                pocket: r.get()?,
            },
            20 => Command::Strip,
            21 => Command::Bind(r.get()?),
            22 => Command::StrikeLoose {
                loose: r.u64()?,
                started: r.f64()?,
            },
            23 => Command::BindLying(r.u64()?),
            24 => Self::StrikeTree {
                tree: r.u64()?,
                height: r.f64()?,
                started: r.f64()?,
            },
            25 => Self::Fasten {
                target: r.get()?,
                fastener: r.get()?,
            },
            26 => Self::Unfasten(r.get()?),
            _ => return Err(r.error("unknown command")),
        })
    }
}

/// An item's index in a store, as written.
fn index(r: &mut Reader<'_>) -> Result<u16, DecodeError> {
    u16::try_from(r.varint()?).map_err(|_| r.error("an item index beyond 16 bits"))
}
