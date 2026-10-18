//! What players did, tick by tick: a play scene's script written down, or a live client's
//! inputs and commands as it made them (`Connection::record`). Replayed into the same
//! scene's world, a recording gives the same state, bit for bit.

use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};
use creeer_math::glam::DVec3;
use creeer_net::message::{ClientMessage, Command, from_bytes, to_bytes};

/// What one player does in one tick: walk (m/s along the ground), jump, and the commands
/// asked for since its last input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Act {
    /// Where to walk.
    pub walk: DVec3,
    /// Jump, if standing.
    pub jump: bool,
    /// Commands, in the order asked.
    pub commands: Vec<Command>,
}

impl Act {
    /// Walking at `walk`, nothing else.
    pub fn walk(walk: DVec3) -> Self {
        Self {
            walk,
            ..Self::default()
        }
    }
}

/// A recording: the scene it was made in, its players, and every tick's acts (one per
/// player, in the players' order).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recording {
    /// The play scene whose world it starts from (`scenes::SCENES`).
    pub scene: String,
    /// The players' names.
    pub players: Vec<String>,
    /// Each tick's acts.
    pub ticks: Vec<Vec<Act>>,
}

/// The first bytes of a recording file.
const MAGIC: &[u8; 8] = b"CREEPLAY";
/// Its format's version.
const VERSION: u32 = 1;
/// The fewest bytes an act takes (its walk, its jump, its count of commands).
const ACT_BYTES: usize = 26;

impl Encode for Act {
    fn encode(&self, w: &mut Writer) {
        // The walk exactly (the network quantises it, the same way on every replay).
        for x in self.walk.to_array() {
            w.f64(x);
        }
        w.u8(u8::from(self.jump));
        w.varint(self.commands.len() as u64);
        for c in &self.commands {
            w.bytes_with_length(&to_bytes(&ClientMessage::Command(c.clone())));
        }
    }
}

impl Decode for Act {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let mut walk = [0.0; 3];
        for x in &mut walk {
            *x = r.f64()?;
        }
        let jump = match r.u8()? {
            0 => false,
            1 => true,
            _ => return Err(r.error("not a flag")),
        };
        let commands = (0..r.count(1)?)
            .map(
                |_| match from_bytes::<ClientMessage>(r.bytes_with_length()?) {
                    Ok(ClientMessage::Command(c)) => Ok(c),
                    _ => Err(r.error("not a command")),
                },
            )
            .collect::<Result<_, _>>()?;
        Ok(Self {
            walk: DVec3::from_array(walk),
            jump,
            commands,
        })
    }
}

impl Encode for Recording {
    fn encode(&self, w: &mut Writer) {
        w.raw(MAGIC);
        w.u32(VERSION);
        w.bytes_with_length(self.scene.as_bytes());
        w.varint(self.players.len() as u64);
        for p in &self.players {
            w.bytes_with_length(p.as_bytes());
        }
        w.varint(self.ticks.len() as u64);
        for tick in &self.ticks {
            for act in tick {
                w.put(act);
            }
        }
    }
}

fn text(r: &mut Reader<'_>) -> Result<String, DecodeError> {
    String::from_utf8(r.bytes_with_length()?.to_vec()).map_err(|_| r.error("text not UTF-8"))
}

impl Decode for Recording {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        if r.raw(MAGIC.len())? != MAGIC {
            return Err(r.error("not a recording"));
        }
        if r.u32()? != VERSION {
            return Err(r.error("a recording of another version"));
        }
        let scene = text(r)?;
        let players = (0..r.count(1)?)
            .map(|_| text(r))
            .collect::<Result<Vec<_>, _>>()?;
        let ticks = (0..r.count(players.len().max(1) * ACT_BYTES)?)
            .map(|_| (0..players.len()).map(|_| r.get()).collect())
            .collect::<Result<_, _>>()?;
        Ok(Self {
            scene,
            players,
            ticks,
        })
    }
}

impl Recording {
    /// The recording as a file's bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        to_bytes(self)
    }

    /// A recording from a file's bytes.
    ///
    /// # Errors
    /// If they are not a whole recording of this version.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        from_bytes(bytes)
    }

    /// One recording from each player's own track (`Connection::take_recording`): ticks
    /// to the shortest track's length.
    pub fn from_tracks(scene: &str, tracks: Vec<(String, Vec<Act>)>) -> Self {
        let len = tracks.iter().map(|(_, t)| t.len()).min().unwrap_or(0);
        let (players, tracks): (Vec<_>, Vec<_>) = tracks.into_iter().unzip();
        let mut ticks = vec![Vec::with_capacity(players.len()); len];
        for track in tracks {
            for (tick, act) in ticks.iter_mut().zip(track) {
                tick.push(act);
            }
        }
        Self {
            scene: scene.to_owned(),
            players,
            ticks,
        }
    }
}
