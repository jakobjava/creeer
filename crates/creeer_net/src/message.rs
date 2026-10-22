//! The messages between a client and the server, and how they are written.
//!
//! Reliable and in order (`channel::RELIABLE`): the handshake, commands, what comes into and
//! out of a client's view, terrain edits and permanent deformation. Unreliable
//! (`channel::UNRELIABLE`): snapshots (`snapshot.rs`, each self-contained against a baseline
//! the client has acknowledged) and the client's inputs (each repeating the last few, so a
//! lost packet loses none).

use crate::entity::EntityId;
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};
use creeer_math::glam::DVec3;
pub use inputs::{THRUST_SCALE, WALK_SCALE};
use inputs::{read_inputs, write_inputs};

/// What an avatar's player asks of it for one tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AvatarInput {
    /// The input's number (one per client tick, from 1).
    pub seq: u32,
    /// Where to walk, in the physics world's frame (its length the speed asked for, m/s;
    /// the part along the ground's normal is ignored).
    pub walk: DVec3,
    /// Jump, if standing on something (swim up, in water).
    pub jump: bool,
    /// The thrusters' push asked for (G1.2c), in the physics world's frame, as a fraction of
    /// the most they push (its length at most 1).
    pub thrust: DVec3,
}

/// No tool, as a wielded tool's index is sent.
pub const NONE: u8 = u8::MAX;

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

/// From a client to the server.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientMessage {
    /// The first message: the protocol it speaks ([`crate::PROTOCOL_VERSION`]), the
    /// player's name and its data.
    Hello {
        protocol: u32,
        name: String,
        /// The hash of its data packs (`creeer_data::Data::hash`): it must be the server's.
        content: u64,
    },
    /// The latest inputs (the last few, newest last) and the newest snapshot received.
    Inputs {
        /// The newest snapshot's tick the client has (0: none yet).
        ack: u32,
        /// The inputs, oldest first.
        inputs: Vec<AvatarInput>,
    },
    /// A command.
    Command(Command),
    /// Send the server's profile (once a second), or stop (F7.6).
    Profile(bool),
}

pub(crate) fn text(w: &mut Writer, s: &str) {
    w.bytes_with_length(s.as_bytes());
}

pub(crate) fn read_text(r: &mut Reader<'_>) -> Result<String, DecodeError> {
    let bytes = r.bytes_with_length()?;
    String::from_utf8(bytes.to_vec()).map_err(|_| r.error("text not UTF-8"))
}

impl Encode for ClientMessage {
    fn encode(&self, w: &mut Writer) {
        match self {
            Self::Hello {
                protocol,
                name,
                content,
            } => {
                w.u8(0);
                w.u32(*protocol);
                text(w, name);
                w.u64(*content);
            }
            Self::Inputs { ack, inputs } => {
                w.u8(1);
                w.varint(u64::from(*ack));
                write_inputs(w, inputs);
            }
            Self::Command(command) => {
                w.u8(2);
                match command {
                    Command::Crate(at) => {
                        w.u8(0);
                        w.put(at);
                    }
                    Command::Ball(at) => {
                        w.u8(1);
                        w.put(at);
                    }
                    Command::Edit(bytes) => {
                        w.u8(2);
                        w.bytes_with_length(bytes);
                    }
                    Command::Clear => w.u8(3),
                    Command::Powerhouse(at) => {
                        w.u8(5);
                        w.put(at);
                    }
                    Command::Frame(at) => {
                        w.u8(4);
                        w.put(at);
                    }
                    Command::Look { skin, hair } => {
                        w.u8(6);
                        w.u8(*skin);
                        w.u8(*hair);
                    }
                    Command::Wield { tool } => {
                        w.u8(7);
                        w.u8(tool.unwrap_or(NONE));
                    }
                    Command::Strike { target, started } => {
                        w.u8(8);
                        w.put(target);
                        w.f64(*started);
                    }
                    Command::Grab(target) => {
                        w.u8(9);
                        w.put(target);
                    }
                    Command::Drop => w.u8(10),
                    Command::Throw(direction) => {
                        w.u8(11);
                        w.put(direction);
                    }
                    Command::Box(at) => {
                        w.u8(12);
                        w.put(at);
                    }
                    Command::Kit { item, at } => {
                        w.u8(13);
                        w.u8(*item);
                        w.put(at);
                    }
                    Command::Stow(target) => {
                        w.u8(14);
                        w.put(target);
                    }
                    Command::TakeOut { pocket, item } => {
                        w.u8(15);
                        w.u8(*pocket);
                        w.varint(u64::from(*item));
                    }
                    Command::PutIn(target) => {
                        w.u8(16);
                        w.put(target);
                    }
                    Command::TakeFrom { container, item } => {
                        w.u8(17);
                        w.put(container);
                        w.varint(u64::from(*item));
                    }
                    Command::Open(target) => {
                        w.u8(18);
                        w.put(target);
                    }
                    Command::Pick { loose, pocket } => {
                        w.u8(19);
                        w.u64(*loose);
                        w.put(pocket);
                    }
                }
            }
            Self::Profile(on) => {
                w.u8(3);
                w.u8(u8::from(*on));
            }
        }
    }
}

impl Decode for ClientMessage {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            0 => Self::Hello {
                protocol: r.u32()?,
                name: read_text(r)?,
                content: r.u64()?,
            },
            1 => Self::Inputs {
                ack: u32::try_from(r.varint()?).map_err(|_| r.error("tick"))?,
                inputs: read_inputs(r)?,
            },
            2 => Self::Command(match r.u8()? {
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
                _ => return Err(r.error("unknown command")),
            }),
            3 => Self::Profile(match r.u8()? {
                0 => false,
                1 => true,
                _ => return Err(r.error("not a flag")),
            }),
            _ => return Err(r.error("unknown client message")),
        })
    }
}

/// An item's index in a store, as written.
fn index(r: &mut Reader<'_>) -> Result<u16, DecodeError> {
    u16::try_from(r.varint()?).map_err(|_| r.error("an item index beyond 16 bits"))
}

/// A message as bytes.
pub fn to_bytes<T: Encode>(message: &T) -> Vec<u8> {
    let mut w = Writer::new();
    w.put(message);
    w.finish()
}

/// A message from bytes.
///
/// # Errors
/// If the bytes are not one whole message.
pub fn from_bytes<T: Decode>(bytes: &[u8]) -> Result<T, DecodeError> {
    let mut r = Reader::new(bytes);
    let message = r.get()?;
    r.end()?;
    Ok(message)
}

mod inputs;
mod inventory;
pub(crate) mod profile;
mod server;
pub use profile::ProfileReport;
pub use server::{Age, Inventory, ItemView, MachineView, Plastic, ServerMessage, StoreView};

#[cfg(test)]
mod tests;
