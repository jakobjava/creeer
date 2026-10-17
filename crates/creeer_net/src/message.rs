//! The messages between a client and the server, and how they are written.
//!
//! Reliable and in order (`channel::RELIABLE`): the handshake, commands, what comes into and
//! out of a client's view, terrain edits and permanent deformation. Unreliable
//! (`channel::UNRELIABLE`): snapshots (`snapshot.rs`, each self-contained against a baseline
//! the client has acknowledged) and the client's inputs (each repeating the last few, so a
//! lost packet loses none).

use crate::entity::{EntityId, Kind};
use crate::quant::Quantised;
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};
use creeer_math::glam::DVec3;

/// What an avatar's player asks of it for one tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AvatarInput {
    /// The input's number (one per client tick, from 1).
    pub seq: u32,
    /// Where to walk, in the physics world's frame (its length the speed asked for, m/s;
    /// the part along the ground's normal is ignored).
    pub walk: DVec3,
    /// Jump, if standing on something.
    pub jump: bool,
}

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
}

/// From a client to the server.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientMessage {
    /// The first message: the protocol it speaks and the player's name.
    Hello {
        /// Its [`crate::PROTOCOL_VERSION`].
        protocol: u32,
        /// The player's name.
        name: String,
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
}

/// A structure's lasting change of shape: nodes now elsewhere relative to node 0 (in its
/// frame; an index past its nodes is a node new to it: a member buckling splits in two),
/// its members anew if they changed, and members now broken.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plastic {
    /// Nodes moved for good: index and where they are now (1/1024 m).
    pub nodes: Vec<(u32, Quantised)>,
    /// All its members, if they changed (a member split where it buckles).
    pub members: Option<Vec<[u32; 2]>>,
    /// Members broken (in the members as they are now).
    pub broken: Vec<u32>,
}

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
        /// The planet the world stands on (its data file, as `creeer_terrain::data_path`
        /// takes it).
        planet: String,
    },
    /// Refused: why (the protocol differs, the server is full).
    Refused(String),
    /// An entity came into view: what it is.
    Spawn(EntityId, Kind),
    /// An entity left view (or the world).
    Despawn(EntityId),
    /// Terrain edits, as `creeer_terrain::edits` writes a log, the first of them the
    /// `first`-th edit of the world (a client joining gets the whole log).
    Edits {
        /// The index of the first edit.
        first: u64,
        /// The edits.
        log: Vec<u8>,
    },
    /// A structure deformed for good.
    Plastic(EntityId, Plastic),
    /// A snapshot (`snapshot.rs`).
    Snapshot(Vec<u8>),
}

/// Steps of an input's walk per m/s (8 mm/s: finer than any hand on a stick).
pub const WALK_SCALE: f64 = 128.0;

const JUMP: u8 = 1;
const SAME_WALK: u8 = 2;
/// Numbered one past the input before it (its number is not written).
const NEXT: u8 = 4;

/// Writes inputs, each against the one before it: its number only if it is not the next, and
/// its walk only if it changed (a byte for an input that walks on as before).
fn write_inputs(w: &mut Writer, inputs: &[AvatarInput]) {
    w.varint(inputs.len() as u64);
    let (mut seq, mut walk) = (0u32, Quantised::default());
    for input in inputs {
        let now = Quantised::new(input.walk, WALK_SCALE);
        let next = seq != 0 && input.seq == seq.wrapping_add(1);
        let flags = if input.jump { JUMP } else { 0 }
            | if now == walk { SAME_WALK } else { 0 }
            | if next { NEXT } else { 0 };
        w.u8(flags);
        if !next {
            w.varint(u64::from(input.seq.wrapping_sub(seq)));
        }
        if now != walk {
            now.write(w, walk);
        }
        (seq, walk) = (input.seq, now);
    }
}

fn read_inputs(r: &mut Reader<'_>) -> Result<Vec<AvatarInput>, DecodeError> {
    let (mut seq, mut walk) = (0u32, Quantised::default());
    (0..r.count(1)?)
        .map(|_| {
            let flags = r.u8()?;
            if flags & !(JUMP | SAME_WALK | NEXT) != 0 {
                return Err(r.error("unknown input flags"));
            }
            let step = if flags & NEXT != 0 {
                1
            } else {
                u32::try_from(r.varint()?).map_err(|_| r.error("input number"))?
            };
            seq = seq
                .checked_add(step)
                .filter(|_| step > 0)
                .ok_or_else(|| r.error("inputs out of order"))?;
            if flags & SAME_WALK == 0 {
                walk = Quantised::read(r, walk)?;
            }
            Ok(AvatarInput {
                seq,
                walk: walk.value(WALK_SCALE),
                jump: flags & JUMP != 0,
            })
        })
        .collect()
}

impl Encode for AvatarInput {
    fn encode(&self, w: &mut Writer) {
        write_inputs(w, std::slice::from_ref(self));
    }
}

impl Decode for AvatarInput {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match read_inputs(r)?.as_slice() {
            [input] => Ok(*input),
            _ => Err(r.error("not one input")),
        }
    }
}

fn text(w: &mut Writer, s: &str) {
    w.bytes_with_length(s.as_bytes());
}

fn read_text(r: &mut Reader<'_>) -> Result<String, DecodeError> {
    let bytes = r.bytes_with_length()?;
    String::from_utf8(bytes.to_vec()).map_err(|_| r.error("text not UTF-8"))
}

impl Encode for ClientMessage {
    fn encode(&self, w: &mut Writer) {
        match self {
            Self::Hello { protocol, name } => {
                w.u8(0);
                w.u32(*protocol);
                text(w, name);
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
                    Command::Frame(at) => {
                        w.u8(4);
                        w.put(at);
                    }
                }
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
                _ => return Err(r.error("unknown command")),
            }),
            _ => return Err(r.error("unknown client message")),
        })
    }
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
            Self::Edits { first, log } => {
                w.u8(4);
                w.varint(*first);
                w.bytes_with_length(log);
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
            }
            Self::Snapshot(bytes) => {
                w.u8(6);
                w.raw(bytes);
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
                Self::Plastic(
                    id,
                    Plastic {
                        nodes,
                        members,
                        broken,
                    },
                )
            }
            6 => Self::Snapshot(r.raw(r.remaining())?.to_vec()),
            _ => return Err(r.error("unknown server message")),
        })
    }
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

#[cfg(test)]
mod tests;
