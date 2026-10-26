//! The messages between a client and the server, and how they are written.
//!
//! Reliable and in order (`channel::RELIABLE`): the handshake, commands, what comes into and
//! out of a client's view, terrain edits and permanent deformation. Unreliable
//! (`channel::UNRELIABLE`): snapshots (`snapshot.rs`, each self-contained against a baseline
//! the client has acknowledged) and the client's inputs (each repeating the last few, so a
//! lost packet loses none).

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
    /// Stopping (G1.10): no walk asked, but one was within the last second: the legs brake at
    /// their most (a body pushed while standing is not braked so).
    pub stopping: bool,
}

/// No tool, as a wielded tool's index is sent.
pub const NONE: u8 = u8::MAX;

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
                w.put(command);
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
            2 => Self::Command(r.get()?),
            3 => Self::Profile(match r.u8()? {
                0 => false,
                1 => true,
                _ => return Err(r.error("not a flag")),
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

mod command;
mod inputs;
pub use command::{Command, Target};
mod inventory;
mod laid;
pub use laid::{ItemLook, Laid, Part};
mod plastic;
pub(crate) mod profile;
mod server;
mod trees;
pub use profile::ProfileReport;
pub use server::{Age, Inventory, ItemView, MachineView, Plastic, ServerMessage, StoreView};
pub use trees::TreeCut;

#[cfg(test)]
mod items_tests;
#[cfg(test)]
mod tests;
