//! The client's side of the game's protocol: its connection to a server (in process or over
//! UDP), and everything the server told it: who it is, what is in its view and how it moves
//! (drawn a little in the past, between snapshots), the terrain's edits, and its own avatar
//! (predicted, `predicted.rs`). The game's window draws from it; tests play with it.

use crate::play::record::Act;
use crate::predicted::Predicted;
use creeer_math::glam::DVec3;
use creeer_net::channel::{RELIABLE, TICK_RATE, UNRELIABLE, config};
use creeer_net::entity::{EntityId, Kind};
use creeer_net::interp::{Playback, Pose, sample};
use creeer_net::link::MemoryEnd;
use creeer_net::message::{ClientMessage, Command, ServerMessage, from_bytes, to_bytes};
use creeer_net::predict::INPUT_EVERY;
use creeer_net::renet::RenetClient;
use creeer_net::snapshot::Receiver;
use creeer_terrain::Terrain;
use creeer_terrain::edits::Edits;
use renet_netcode::NetcodeClientTransport;
use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// How far behind the server the client draws (ticks: two snapshots, 67 ms).
pub const DELAY: f64 = 4.0;

/// Where the connection stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Connecting, or waiting for the server's welcome.
    Connecting,
    /// In the game.
    Joined,
    /// The server refused it (why).
    Refused(String),
    /// The connection is gone (why).
    Closed(String),
}

/// What the server said of the client when it joined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Welcome {
    /// The client's id.
    pub client: u64,
    /// Its avatar.
    pub avatar: EntityId,
    /// The planet's data file.
    pub planet: String,
}

enum Link {
    Memory(MemoryEnd),
    Udp(Box<NetcodeClientTransport>),
}

/// A connection to a server.
pub struct Connection {
    renet: RenetClient,
    link: Link,
    name: String,
    /// The hash of its data packs, sent in its hello.
    content: u64,
    said_hello: bool,
    status: Status,
    /// What the server said when the client joined.
    pub welcome: Option<Welcome>,
    /// What each entity in view is.
    pub kinds: BTreeMap<EntityId, Kind>,
    /// Each entity's age, as the server last told it (G1.3c).
    pub ages: BTreeMap<EntityId, creeer_net::message::Age>,
    /// The members of structures that are broken.
    pub broken: BTreeMap<EntityId, BTreeSet<u32>>,
    /// The world's machines as the server last told them (F7.4).
    pub machines: Vec<creeer_net::message::MachineView>,
    /// The server's profile, as its last report gave it (while asked for: `watch_profile`).
    pub profile: Option<creeer_net::message::ProfileReport>,
    /// The jetpack's gas, as the server last told it (kg).
    pub gas: f64,
    /// Its health (a share of a whole one), and whether it is down (its ragdoll, drawn as the
    /// server has it: not predicted), as the server last told them.
    pub health: f64,
    pub fallen: bool,
    /// The terrain's edits, as the server has them.
    pub edits: Edits,
    /// Changes whenever edits were replaced rather than added to.
    pub edits_generation: u64,
    receiver: Receiver,
    playback: Playback,
    drawn_at: f64,
    terrain: Option<Arc<Terrain>>,
    pub(crate) predicted: Option<Predicted>,
    /// Whether this tick is a send round (every [`INPUT_EVERY`]: the inputs, and renet's
    /// acknowledgements with them, go out together).
    due: bool,
    /// What it sent, act by act, while recording (`record`), and the commands since the
    /// last input.
    recording: Option<(Vec<Act>, Vec<Command>)>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("status", &self.status)
            .field("entities", &self.kinds.len())
            .finish_non_exhaustive()
    }
}

impl Connection {
    fn with(renet: RenetClient, link: Link, name: &str, content: u64) -> Self {
        Self {
            renet,
            link,
            name: name.to_owned(),
            content,
            said_hello: false,
            status: Status::Connecting,
            welcome: None,
            kinds: BTreeMap::new(),
            broken: BTreeMap::new(),
            ages: BTreeMap::new(),
            machines: Vec::new(),
            profile: None,
            gas: creeer_sim::character::TANK,
            health: 1.0,
            fallen: false,
            edits: Edits::default(),
            edits_generation: 0,
            receiver: Receiver::default(),
            playback: Playback::new(DELAY),
            drawn_at: 0.0,
            terrain: None,
            predicted: None,
            due: true,
            recording: None,
        }
    }

    /// Records from now on every input and command it sends (a play scene's track, or a
    /// live player's: `play::record`).
    pub fn record(&mut self) {
        self.recording = Some((Vec::new(), Vec::new()));
    }

    /// What it recorded since `record` (and goes on recording).
    pub fn take_recording(&mut self) -> Vec<Act> {
        self.recording
            .as_mut()
            .map(|(acts, _)| std::mem::take(acts))
            .unwrap_or_default()
    }

    /// Joins as `name` (data of hash `content`) through `end` (`Server::connect_memory`).
    pub fn memory(end: MemoryEnd, name: &str, content: u64) -> Self {
        let mut renet = RenetClient::new(config());
        renet.set_connected();
        Self::with(renet, Link::Memory(end), name, content)
    }

    /// Joins as `name` the server at `server` over UDP.
    ///
    /// # Errors
    /// If no socket can be opened.
    pub fn udp(server: SocketAddr, name: &str, content: u64) -> Result<Self, String> {
        let (renet, transport) = creeer_net::udp::connect(server, creeer_net::udp::fresh_id())
            .map_err(|e| e.to_string())?;
        Ok(Self::with(
            renet,
            Link::Udp(Box::new(transport)),
            name,
            content,
        ))
    }

    /// Where it stands.
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Gives it the terrain of the planet it plays on (the one the welcome names): the edits
    /// so far are applied to it, and the avatar is predicted on it from now on.
    pub fn attach_terrain(&mut self, terrain: Arc<Terrain>, data: &creeer_data::Data) {
        terrain.set_edits(self.edits.clone());
        // The planet's air and sea, as the server's world has them (its packs are this one's).
        let around = creeer_materials::species::SpeciesTable::from_data(data)
            .ok()
            .and_then(|species| {
                creeer_sim::surroundings::Surroundings::of(terrain.planet(), &species).ok()
            });
        self.predicted = Some(Predicted::new(Arc::clone(&terrain), around));
        self.terrain = Some(terrain);
    }

    /// `dt` passed: takes in what the server sent.
    pub fn update(&mut self, dt: Duration) {
        self.renet.update(dt);
        match &mut self.link {
            Link::Memory(end) => {
                for packet in end.receive() {
                    self.renet.process_packet(&packet);
                }
                if end.is_closed() {
                    self.renet.disconnect_due_to_transport();
                }
            }
            Link::Udp(transport) => {
                if let Err(e) = transport.update(dt, &mut self.renet) {
                    self.close(format!("network: {e}"));
                }
            }
        }
        if self.renet.is_disconnected() {
            let why = self
                .renet
                .disconnect_reason()
                .map_or("disconnected".into(), |r| r.to_string());
            self.close(why);
        }
        if self.renet.is_connected() && !self.said_hello {
            self.said_hello = true;
            let hello = ClientMessage::Hello {
                protocol: creeer_net::PROTOCOL_VERSION,
                name: self.name.clone(),
                content: self.content,
            };
            self.renet.send_message(RELIABLE, to_bytes(&hello));
        }
        while let Some(bytes) = self.renet.receive_message(RELIABLE) {
            if let Ok(message) = from_bytes::<ServerMessage>(&bytes) {
                self.handle(message);
            }
        }
        while let Some(bytes) = self.renet.receive_message(UNRELIABLE) {
            // (Its own character's state comes this way too, with every round.)
            if let Ok(m @ (ServerMessage::Snapshot(_) | ServerMessage::Own { .. })) =
                from_bytes::<ServerMessage>(&bytes)
            {
                self.handle(m);
            }
        }
        let newest = self.receiver.newest().map(|s| s.tick);
        self.drawn_at = self
            .playback
            .advance(dt.as_secs_f64() * f64::from(TICK_RATE), newest);
        if let Some(p) = &mut self.predicted {
            p.advance(dt.as_secs_f64());
        }
    }

    fn close(&mut self, why: String) {
        if !matches!(self.status, Status::Refused(_) | Status::Closed(_)) {
            self.status = Status::Closed(why);
        }
    }

    /// One client tick of input: walk at `walk` (m/s, along the ground), jump or not, the
    /// jetpack's `thrust` (a fraction of its most). It moves the predicted avatar at once and
    /// goes to the server (with the last few).
    pub fn input(&mut self, walk: DVec3, jump: bool, thrust: DVec3) {
        let Some(p) = &mut self.predicted else { return };
        if let Some((acts, commands)) = &mut self.recording {
            acts.push(Act {
                walk,
                jump,
                thrust,
                commands: std::mem::take(commands),
            });
        }
        let input = p.input(walk, jump, thrust);
        if input.seq % INPUT_EVERY != 0 {
            return;
        }
        self.due = true;
        let message = ClientMessage::Inputs {
            ack: self.receiver.newest().map_or(0, |s| s.tick),
            inputs: p.to_send(),
        };
        self.renet.send_message(UNRELIABLE, to_bytes(&message));
    }

    /// Asks the world for something.
    pub fn command(&mut self, command: Command) {
        if let Some((_, commands)) = &mut self.recording {
            commands.push(command.clone());
        }
        self.renet
            .send_message(RELIABLE, to_bytes(&ClientMessage::Command(command)));
        self.due = true;
    }

    /// Asks the server for its profile, a report a second (`on`), or for no more.
    pub fn watch_profile(&mut self, on: bool) {
        self.renet
            .send_message(RELIABLE, to_bytes(&ClientMessage::Profile(on)));
        if !on {
            self.profile = None;
        }
        self.due = true;
    }

    /// Sends what it has to send, in a send round (or until it plays: at once).
    pub fn flush(&mut self) {
        if self.status == Status::Joined && self.predicted.is_some() && !self.due {
            return;
        }
        self.due = false;
        match &mut self.link {
            Link::Memory(end) => {
                if !end.send(self.renet.get_packets_to_send()) {
                    self.close("the server is gone".into());
                }
            }
            Link::Udp(transport) => {
                if let Err(e) = transport.send_packets(&mut self.renet) {
                    self.close(format!("network: {e}"));
                }
            }
        }
    }

    /// Every entity's pose to draw it at now (the player's own avatar where it is predicted).
    pub fn poses(&self) -> BTreeMap<EntityId, Pose> {
        let mut poses = sample(&self.receiver, self.drawn_at, 1.0 / f64::from(TICK_RATE));
        poses.retain(|id, _| self.kinds.contains_key(id));
        if self.fallen {
            return poses;
        }
        let Some(p) = &self.predicted else {
            return poses;
        };
        // Bodies just out of the prediction slide to where they are interpolated; those in it
        // are drawn as predicted (ADR-0025).
        for (id, pose) in &mut poses {
            pose.position += p.released(*id);
        }
        for (id, pose) in p.nearby() {
            if poses.contains_key(&id) {
                poses.insert(id, pose);
            }
        }
        if let (Some(w), Some(pose)) = (&self.welcome, p.pose()) {
            poses.insert(w.avatar, pose);
        }
        poses
    }

    /// How far the prediction of the player's avatar was corrected.
    pub fn corrections(&self) -> crate::predicted::Corrections {
        self.predicted
            .as_ref()
            .map(|p| p.corrections)
            .unwrap_or_default()
    }

    /// The player's own avatar as drawn, once it has come.
    pub fn avatar(&self) -> Option<Pose> {
        if self.fallen {
            // Down: its pelvis as the server has it.
            let pelvis = self.welcome.as_ref()?.avatar;
            let poses = sample(&self.receiver, self.drawn_at, 1.0 / f64::from(TICK_RATE));
            return poses.get(&pelvis).copied();
        }
        self.predicted.as_ref()?.pose()
    }

    /// The server tick drawn at (fractional; a little in the past).
    pub fn drawn_at(&self) -> f64 {
        self.drawn_at
    }

    /// How the connection fares: round trip, loss, bytes a second each way.
    pub fn network(&self) -> creeer_net::renet::NetworkInfo {
        self.renet.network_info()
    }

    /// The terrain it was given.
    pub fn terrain(&self) -> Option<&Arc<Terrain>> {
        self.terrain.as_ref()
    }

    /// The snapshots received (for tests and diagnostics).
    pub fn receiver(&self) -> &Receiver {
        &self.receiver
    }
}

mod apply;
mod receive;
