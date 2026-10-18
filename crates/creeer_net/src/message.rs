//! The messages between a client and the server, and how they are written.
//!
//! Reliable and in order (`channel::RELIABLE`): the handshake, commands, what comes into and
//! out of a client's view, terrain edits and permanent deformation. Unreliable
//! (`channel::UNRELIABLE`): snapshots (`snapshot.rs`, each self-contained against a baseline
//! the client has acknowledged) and the client's inputs (each repeating the last few, so a
//! lost packet loses none).

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
    /// Build a powerhouse (F6.8's machine) at a point.
    Powerhouse(DVec3),
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
}

/// Numbered one past the input before it (its number is not written).
/// Steps of an input's walk per m/s (8 mm/s: finer than any hand on a stick).
pub const WALK_SCALE: f64 = 128.0;

const JUMP: u8 = 1;
const SAME_WALK: u8 = 2;
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
                _ => return Err(r.error("unknown command")),
            }),
            _ => return Err(r.error("unknown client message")),
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

mod server;
pub use server::{MachineView, Plastic, ServerMessage};

#[cfg(test)]
mod tests;
