//! The client's side of the game's protocol: its connection to a server (in process or over
//! UDP), and everything the server told it: who it is, what is in its view and how it moves
//! (drawn a little in the past, between snapshots), the terrain's edits, and its own avatar
//! (predicted, `predicted.rs`). The game's window draws from it; tests play with it.

use crate::predicted::Predicted;
use creeer_math::glam::DVec3;
use creeer_net::channel::{RELIABLE, TICK_RATE, UNRELIABLE, config};
use creeer_net::entity::{EntityId, Kind};
use creeer_net::interp::{Playback, Pose, sample};
use creeer_net::link::MemoryEnd;
use creeer_net::message::{ClientMessage, Command, Plastic, ServerMessage, from_bytes, to_bytes};
use creeer_net::predict::INPUT_EVERY;
use creeer_net::quant::POSITION_SCALE;
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
    said_hello: bool,
    status: Status,
    /// What the server said when the client joined.
    pub welcome: Option<Welcome>,
    /// What each entity in view is.
    pub kinds: BTreeMap<EntityId, Kind>,
    /// The members of structures that are broken.
    pub broken: BTreeMap<EntityId, BTreeSet<u32>>,
    /// The terrain's edits, as the server has them.
    pub edits: Edits,
    /// Changes whenever edits were replaced rather than added to.
    pub edits_generation: u64,
    receiver: Receiver,
    playback: Playback,
    drawn_at: f64,
    terrain: Option<Arc<Terrain>>,
    predicted: Option<Predicted>,
    /// Whether this tick is a send round (every [`INPUT_EVERY`]: the inputs, and renet's
    /// acknowledgements with them, go out together).
    due: bool,
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
    fn with(renet: RenetClient, link: Link, name: &str) -> Self {
        Self {
            renet,
            link,
            name: name.to_owned(),
            said_hello: false,
            status: Status::Connecting,
            welcome: None,
            kinds: BTreeMap::new(),
            broken: BTreeMap::new(),
            edits: Edits::default(),
            edits_generation: 0,
            receiver: Receiver::default(),
            playback: Playback::new(DELAY),
            drawn_at: 0.0,
            terrain: None,
            predicted: None,
            due: true,
        }
    }

    /// Joins as `name` through the in-process connection `end` (`Server::connect_memory`).
    pub fn memory(end: MemoryEnd, name: &str) -> Self {
        let mut renet = RenetClient::new(config());
        renet.set_connected();
        Self::with(renet, Link::Memory(end), name)
    }

    /// Joins as `name` the server at `server` over UDP.
    ///
    /// # Errors
    /// If no socket can be opened.
    pub fn udp(server: SocketAddr, name: &str) -> Result<Self, String> {
        let (renet, transport) = creeer_net::udp::connect(server, creeer_net::udp::fresh_id())
            .map_err(|e| e.to_string())?;
        Ok(Self::with(renet, Link::Udp(Box::new(transport)), name))
    }

    /// Where it stands.
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Gives it the terrain of the planet it plays on (the one the welcome names): the edits
    /// so far are applied to it, and the avatar is predicted on it from now on.
    pub fn attach_terrain(&mut self, terrain: Arc<Terrain>) {
        terrain.set_edits(self.edits.clone());
        self.predicted = Some(Predicted::new(Arc::clone(&terrain)));
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
            };
            self.renet.send_message(RELIABLE, to_bytes(&hello));
        }
        while let Some(bytes) = self.renet.receive_message(RELIABLE) {
            if let Ok(message) = from_bytes::<ServerMessage>(&bytes) {
                self.handle(message);
            }
        }
        while let Some(bytes) = self.renet.receive_message(UNRELIABLE) {
            if let Ok(ServerMessage::Snapshot(snapshot)) = from_bytes::<ServerMessage>(&bytes) {
                self.snapshot(&snapshot);
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

    fn handle(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Welcome {
                client,
                avatar,
                planet,
                ..
            } => {
                self.status = Status::Joined;
                self.welcome = Some(Welcome {
                    client,
                    avatar,
                    planet,
                });
            }
            ServerMessage::Refused(why) => self.status = Status::Refused(why),
            ServerMessage::Spawn(id, kind) => {
                self.broken.remove(&id);
                self.kinds.insert(id, kind);
            }
            ServerMessage::Despawn(id) => {
                self.kinds.remove(&id);
                self.broken.remove(&id);
            }
            ServerMessage::Edits { first, log } => self.edited(first, &log),
            ServerMessage::Plastic(id, plastic) => self.deformed(id, &plastic),
            ServerMessage::Snapshot(snapshot) => self.snapshot(&snapshot),
        }
    }

    /// Edits from the `first`-th on: the client keeps the edits before it, and these
    /// replace the rest.
    fn edited(&mut self, first: u64, log: &[u8]) {
        let Ok(part) = Edits::from_bytes(log) else {
            return;
        };
        let Ok(first) = usize::try_from(first) else {
            return;
        };
        if first > self.edits.len() {
            return; // never, in order: a gap would be a server's mistake
        }
        let replaced = first < self.edits.len();
        let mut edits = Edits::default();
        for edit in self.edits.log()[..first].iter().chain(part.log()) {
            edits.push(*edit);
        }
        self.edits = edits;
        if replaced {
            self.edits_generation += 1;
        }
        if let Some(terrain) = &self.terrain {
            if replaced {
                terrain.set_edits(self.edits.clone());
            } else {
                for edit in part.log() {
                    terrain.edit(*edit);
                }
            }
        }
    }

    fn deformed(&mut self, id: EntityId, plastic: &Plastic) {
        let Some(Kind::Structure(frame)) = self.kinds.get_mut(&id) else {
            return;
        };
        for (node, at) in &plastic.nodes {
            let node = *node as usize;
            if node >= frame.nodes.len() {
                frame.nodes.resize(node + 1, DVec3::ZERO);
            }
            frame.nodes[node] = at.value(POSITION_SCALE);
        }
        if let Some(members) = &plastic.members {
            frame.members.clone_from(members);
            self.broken.remove(&id);
        }
        if !plastic.broken.is_empty() {
            self.broken.entry(id).or_default().extend(&plastic.broken);
        }
    }

    fn snapshot(&mut self, bytes: &[u8]) {
        let Ok(snapshot) = self.receiver.read(bytes) else {
            return;
        };
        let (tick, last_input) = (snapshot.tick, snapshot.last_input);
        let mine = self
            .welcome
            .as_ref()
            .and_then(|w| snapshot.states.get(&w.avatar))
            .copied();
        if let (Some(state), Some(p)) = (mine, &mut self.predicted) {
            p.reconcile(tick, &state, last_input);
        }
    }

    /// One client tick of input: walk at `walk` (m/s, along the ground), jump or not. It
    /// moves the predicted avatar at once and goes to the server (with the last few).
    pub fn input(&mut self, walk: DVec3, jump: bool) {
        let Some(p) = &mut self.predicted else { return };
        let input = p.input(walk, jump);
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
        self.renet
            .send_message(RELIABLE, to_bytes(&ClientMessage::Command(command)));
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
        if let (Some(w), Some(pose)) = (
            &self.welcome,
            self.predicted.as_ref().and_then(Predicted::pose),
        ) {
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
