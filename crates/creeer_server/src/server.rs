//! The server: the world, the sessions of the clients connected to it, and their
//! connections (renet's, carried in process for a singleplayer game and over UDP for a
//! dedicated server).
//!
//! A tick: what came from every client is taken in (a player joining, commands, inputs);
//! every avatar is steered by its player's input for the tick; the world steps; and every
//! [`SEND_EVERY`] ticks each client is sent what its view and budget call for
//! (`session.rs`).

use crate::replicate::Shapes;
use crate::session::Session;
use crate::world::{DT, ServerWorld};
use creeer_net::PROTOCOL_VERSION;
use creeer_net::channel::{RELIABLE, SEND_EVERY, TICK_RATE, UNRELIABLE, config};
use creeer_net::interest::Grid;
use creeer_net::link::{MemoryEnd, memory};
use creeer_net::message::{ClientMessage, ServerMessage, from_bytes, to_bytes};
use creeer_net::renet::{ClientId, RenetServer, ServerEvent};
use renet_netcode::NetcodeServerTransport;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;

/// The longest name a player may have (characters).
pub const LONGEST_NAME: usize = 32;
/// Ticks between the machines' list sent to every client (F7.4).
pub const MACHINES_EVERY: u32 = 60;
/// Ticks a refused client is given to hear why before it is disconnected.
const CLOSE_AFTER: u64 = 30;

/// What happened that the host may want to show (joins, leaves, refusals).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A player joined.
    Joined(String),
    /// A player left.
    Left(String),
    /// A client was refused, and why.
    Refused(ClientId, String),
    /// A command was refused, and why.
    CommandRefused(String, String),
}

/// The server.
pub struct Server {
    /// The world.
    pub world: ServerWorld,
    renet: RenetServer,
    sessions: BTreeMap<ClientId, Session>,
    shapes: Shapes,
    memory: Vec<(ClientId, MemoryEnd)>,
    udp: Option<NetcodeServerTransport>,
    closing: Vec<(ClientId, u64)>,
    events: Vec<Event>,
    next_memory: ClientId,
    /// Ticks since the server started: what snapshots are numbered by (the world's own
    /// tick goes back when a save is loaded; a connection's must not).
    net_tick: u32,
}

impl std::fmt::Debug for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Server")
            .field("tick", &self.world.tick)
            .field("sessions", &self.sessions.len())
            .finish_non_exhaustive()
    }
}

impl Server {
    /// A server for `world`, with no one connected.
    pub fn new(world: ServerWorld) -> Self {
        Self {
            world,
            renet: RenetServer::new(config()),
            sessions: BTreeMap::new(),
            shapes: Shapes::default(),
            memory: Vec::new(),
            udp: None,
            closing: Vec::new(),
            events: Vec::new(),
            next_memory: 1,
            net_tick: 0,
        }
    }

    /// A connection in process (a singleplayer game's): the client's end of it.
    pub fn connect_memory(&mut self) -> MemoryEnd {
        // Ids in the top 2³² (a UDP client picking one by chance: one in 2³²), in order.
        let id = (u64::MAX - u64::from(u32::MAX)) + self.next_memory;
        self.next_memory += 1;
        let (server_end, client_end) = memory();
        self.renet.add_connection(id);
        self.memory.push((id, server_end));
        client_end
    }

    /// Listens for clients over UDP at `addr` (port 0: any free one); returns where.
    ///
    /// # Errors
    /// If the socket cannot be bound.
    pub fn listen(&mut self, addr: SocketAddr) -> std::io::Result<SocketAddr> {
        let (_, transport) = creeer_net::udp::listen(addr)?;
        let at = transport.addresses()[0];
        self.udp = Some(transport);
        Ok(at)
    }

    /// What happened since the last call.
    pub fn events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The players in the game.
    pub fn players(&self) -> Vec<&str> {
        self.sessions
            .values()
            .filter_map(|s| s.player.as_deref())
            .collect()
    }

    /// One tick.
    pub fn tick(&mut self) {
        let dt = Duration::from_secs_f64(DT);
        self.renet.update(dt);
        if let Some(udp) = &mut self.udp
            && let Err(e) = udp.update(dt, &mut self.renet)
        {
            self.events.push(Event::Refused(0, format!("network: {e}")));
        }
        for (id, end) in &self.memory {
            for packet in end.receive() {
                // A connection it no longer has: nothing to hand it to.
                let _ = self.renet.process_packet_from(&packet, *id);
            }
        }
        self.connections();
        self.receive();
        let mut steered = std::collections::HashSet::new();
        for session in self.sessions.values_mut() {
            if let Some(avatar) = session.avatar {
                let input = session.inputs.take();
                self.world.steer(avatar, &input);
                steered.insert(avatar);
            }
        }
        // Avatars whose players left stand still (the last push would carry them on).
        for id in self.world.avatars() {
            if !steered.contains(&id) {
                self.world
                    .steer(id, &creeer_net::message::AvatarInput::default());
            }
        }
        let centres: Vec<_> = (self.sessions.values())
            .filter_map(|s| s.avatar.and_then(|id| self.world.index(id)))
            .map(|i| self.world.world.bodies[i].structure.position[0])
            .collect();
        self.world.regions(&centres);
        self.world.step();
        self.net_tick += 1;
        if self.net_tick.is_multiple_of(MACHINES_EVERY) {
            let views = ServerMessage::Machines(self.world.machine_views());
            for session in self.sessions.values_mut().filter(|s| s.player.is_some()) {
                session.queue(&views);
            }
        }
        if self.net_tick.is_multiple_of(SEND_EVERY) {
            self.send();
        }
        let tick = self.world.tick;
        let (close, keep): (Vec<_>, Vec<_>) = self
            .closing
            .drain(..)
            .partition(|c| tick >= c.1 + CLOSE_AFTER);
        self.closing = keep;
        for (id, _) in close {
            self.renet.disconnect(id);
        }
        // In process, the server is the transport: it sends, and removes the connections it
        // disconnected or whose client is gone.
        let renet = &mut self.renet;
        let ended = renet.disconnections_id();
        self.memory.retain(|(id, end)| {
            let packets = renet.get_packets_to_send(*id).unwrap_or_default();
            let open = end.send(packets) && !end.is_closed() && !ended.contains(id);
            if !open {
                renet.remove_connection(*id);
            }
            open
        });
        if let Some(udp) = &mut self.udp {
            udp.send_packets(&mut self.renet);
        }
    }

    /// Tells every player the world anew, after it was replaced: their avatar in it, and
    /// everything in it.
    pub(crate) fn restart_sessions(&mut self) {
        self.shapes = Shapes::default();
        let tick = self.net_tick;
        for session in self.sessions.values_mut() {
            session.restart();
            let Some(player) = session.player.clone() else {
                continue;
            };
            let avatar = self.world.avatar_of(&player);
            session.avatar = Some(avatar);
            session.queue(&ServerMessage::Welcome {
                client: session.client,
                avatar,
                tick,
                rate: TICK_RATE,
                planet: self.world.planet.clone(),
            });
        }
    }

    fn connections(&mut self) {
        while let Some(event) = self.renet.get_event() {
            match event {
                ServerEvent::ClientConnected { client_id } => {
                    self.sessions.insert(client_id, Session::new(client_id));
                }
                ServerEvent::ClientDisconnected { client_id, .. } => {
                    // Its avatar stays where it is, for when the player comes again.
                    if let Some(player) = self.sessions.remove(&client_id).and_then(|s| s.player) {
                        self.events.push(Event::Left(player));
                    }
                }
            }
        }
    }

    fn receive(&mut self) {
        let ids: Vec<ClientId> = self.sessions.keys().copied().collect();
        for id in ids {
            while let Some(bytes) = self.renet.receive_message(id, RELIABLE) {
                match from_bytes::<ClientMessage>(&bytes) {
                    Ok(ClientMessage::Hello {
                        protocol,
                        name,
                        content,
                    }) => self.hello(id, protocol, &name, content),
                    Ok(ClientMessage::Command(command)) => {
                        let Some(session) = self.sessions.get_mut(&id) else {
                            continue;
                        };
                        let (Some(player), Some(avatar)) = (session.player.clone(), session.avatar)
                        else {
                            continue;
                        };
                        let result = if session.may_command() {
                            self.world.apply(&command, Some(avatar))
                        } else {
                            Err("too many commands".into())
                        };
                        if let Err(why) = result {
                            self.events.push(Event::CommandRefused(player, why));
                        }
                    }
                    Ok(ClientMessage::Inputs { .. }) | Err(_) => {}
                }
            }
            while let Some(bytes) = self.renet.receive_message(id, UNRELIABLE) {
                if let (Ok(ClientMessage::Inputs { ack, inputs }), Some(session)) = (
                    from_bytes::<ClientMessage>(&bytes),
                    self.sessions.get_mut(&id),
                ) {
                    session.acknowledged(ack);
                    if session.avatar.is_some() {
                        session.inputs.receive(&inputs);
                    }
                }
            }
        }
    }

    fn hello(&mut self, id: ClientId, protocol: u32, name: &str, content: u64) {
        let name = name.trim();
        let playing = self
            .sessions
            .values()
            .any(|s| s.player.as_deref() == Some(name));
        let refusal = if protocol != PROTOCOL_VERSION {
            Some(format!(
                "this server speaks protocol {PROTOCOL_VERSION}, the client {protocol}"
            ))
        } else if content != self.world.data.hash() {
            Some(different_data(&self.world.data, content))
        } else if name.is_empty() || name.chars().count() > LONGEST_NAME {
            Some(format!("a name of 1 to {LONGEST_NAME} characters, please"))
        } else if playing {
            Some(format!("{name} is playing already"))
        } else {
            None
        };
        let Some(session) = self.sessions.get_mut(&id) else {
            return;
        };
        if session.player.is_some() {
            return;
        }
        if let Some(why) = refusal {
            self.renet
                .send_message(id, RELIABLE, to_bytes(&ServerMessage::Refused(why.clone())));
            self.closing.push((id, self.world.tick));
            self.events.push(Event::Refused(id, why));
            return;
        }
        let avatar = self.world.avatar_of(name);
        session.player = Some(name.to_owned());
        session.avatar = Some(avatar);
        let tick = self.net_tick;
        session.queue(&ServerMessage::Welcome {
            client: id,
            avatar,
            tick,
            rate: TICK_RATE,
            planet: self.world.planet.clone(),
        });
        self.events.push(Event::Joined(name.to_owned()));
    }

    fn send(&mut self) {
        let world = &self.world;
        let at = world
            .ids()
            .iter()
            .zip(&world.world.bodies)
            .map(|(id, b)| (*id, b.structure.position[0]));
        let grid = Grid::new(at);
        let bodies = world.ids().iter().copied().zip(&world.world.bodies);
        let changes = self.shapes.changes(bodies, world.tick);
        let seconds = f64::from(SEND_EVERY) * DT;
        for session in self.sessions.values_mut().filter(|s| s.player.is_some()) {
            session.update_view(world, &grid, &mut self.shapes);
            for (id, change) in &changes {
                session.plastic(*id, change);
            }
            session.send(&mut self.renet, world, seconds, self.net_tick);
        }
    }
}

#[cfg(test)]
mod play_tests;
#[cfg(test)]
pub(crate) mod tests;

/// Why a client with data `content` may not join a server with `data`: the server's packs.
fn different_data(data: &creeer_data::Data, content: u64) -> String {
    let packs: Vec<String> = data
        .packs()
        .iter()
        .map(|p| format!("{} {}", p.name(), p.manifest.version))
        .collect();
    format!(
        "this server's data differs from yours (its packs: {}; content {:016x}, yours {content:016x})",
        packs.join(", "),
        data.hash()
    )
}
