//! A client's session: who it is, its avatar and inputs, what it has been told, and what it
//! is sent each round within its budget.
//!
//! Every send round the session works out its view (what lies within [`VIEW`] of its avatar;
//! what it has stays until beyond [`VIEW`] · [`LEAVE`], so nothing at the edge comes and goes
//! every round), queues what came into view (spawns), what left it (despawns), the terrain
//! edits it has not had and the shape changes of what it has; the pacer lets out of that
//! queue what the budget pays for (`creeer_net::channel`), and the snapshot takes the rest of
//! the budget: the states of what the client has been told of, most important first.

use crate::replicate::{Shapes, state};
use crate::world::ServerWorld;
use creeer_math::glam::DVec3;
use creeer_net::channel::{Pacer, RELIABLE, UNRELIABLE};
use creeer_net::entity::EntityId;
use creeer_net::interest::{Grid, importance};
use creeer_net::message::{Age, Plastic, ServerMessage, to_bytes};
use creeer_net::predict::{InputQueue, NEARBY};
use creeer_net::renet::{ClientId, RenetServer};
use creeer_net::snapshot::Sender;
use creeer_terrain::edits::Edits;
use std::collections::{HashSet, VecDeque};

/// How far a client sees (m).
pub const VIEW: f64 = 600.0;
/// What it has leaves its view only this much farther out.
pub const LEAVE: f64 = 1.1;
/// Edits per message (each at most 59 bytes: a message under 1 kB).
pub const EDITS_PER_MESSAGE: usize = 16;
/// Commands a client may give a second, and at once.
pub const COMMANDS_PER_SECOND: f64 = 5.0;
/// See [`COMMANDS_PER_SECOND`].
pub const COMMAND_BURST: f64 = 10.0;

/// A message waiting to be sent, and the entity it brings into view if it is a spawn.
#[derive(Debug)]
struct Queued {
    bytes: Vec<u8>,
    spawns: Option<EntityId>,
}

/// One client's session.
#[derive(Debug)]
pub struct Session {
    /// The connection.
    pub client: ClientId,
    /// The player, once they said who they are.
    pub player: Option<String>,
    /// Their avatar, once they have one.
    pub avatar: Option<EntityId>,
    /// Their inputs as they come.
    pub inputs: InputQueue,
    sender: Sender,
    pacer: Pacer,
    outbox: VecDeque<Queued>,
    /// What the client has been sent the spawn of.
    spawned: HashSet<EntityId>,
    /// What it will be, once the queue lets the spawn out.
    pending: HashSet<EntityId>,
    edits_told: usize,
    /// How many of the loose things picked up it has been told (G1.6c).
    taken_told: usize,
    commands: f64,
}

impl Session {
    /// A session for connection `client`.
    pub fn new(client: ClientId) -> Self {
        Self {
            client,
            player: None,
            avatar: None,
            inputs: InputQueue::default(),
            sender: Sender::default(),
            pacer: Pacer::default(),
            outbox: VecDeque::new(),
            spawned: HashSet::new(),
            pending: HashSet::new(),
            edits_told: 0,
            taken_told: 0,
            commands: COMMAND_BURST,
        }
    }

    /// The newest snapshot the client says it has.
    pub fn acknowledged(&mut self, tick: u32) {
        self.sender.ack(tick);
    }

    /// Whether it may give a command now (and counts it).
    pub fn may_command(&mut self) -> bool {
        if self.commands >= 1.0 {
            self.commands -= 1.0;
            true
        } else {
            false
        }
    }

    /// Queues a reliable message.
    pub fn queue(&mut self, message: &ServerMessage) {
        self.outbox.push_back(Queued {
            bytes: to_bytes(message),
            spawns: None,
        });
    }

    /// The world was replaced: what was queued of the old one goes, what the client has of
    /// it is despawned, and its edits are sent again from the first (which replace its own).
    pub fn restart(&mut self) {
        self.outbox.clear();
        self.pending.clear();
        let mut gone: Vec<EntityId> = self.spawned.drain().collect();
        gone.sort_unstable();
        for id in gone {
            self.queue(&ServerMessage::Despawn(id));
        }
        self.edits_told = 0;
        self.taken_told = 0;
        self.inputs = InputQueue::default();
    }
    /// Entity `id` became something else (a character fallen, or up again): if the client
    /// has it, it is despawned, to be spawned as it is now with the view's next update.
    pub fn forget(&mut self, id: EntityId) {
        if self.pending.remove(&id) {
            self.outbox.retain(|q| q.spawns != Some(id));
        } else if self.spawned.remove(&id) {
            self.queue(&ServerMessage::Despawn(id));
        }
    }

    /// Whether the client has (or is about to have) entity `id`.
    pub fn has(&self, id: EntityId) -> bool {
        self.spawned.contains(&id) || self.pending.contains(&id)
    }

    /// Where the client looks from: its avatar, or where avatars appear.
    fn centre(&self, world: &ServerWorld) -> DVec3 {
        self.avatar
            .and_then(|id| world.index(id))
            .map_or(world.spawn(), |i| {
                world.world.bodies[i].structure.position[0]
            })
    }

    /// Queues what came into and left the client's view, and the edits it has not had.
    pub fn update_view(
        &mut self,
        world: &ServerWorld,
        grid: &Grid,
        shapes: &mut Shapes,
        tick: u32,
    ) {
        let centre = self.centre(world);
        // One search for both: what may stay, and of that what is near enough to come.
        let around = grid.around(centre, VIEW * LEAVE);
        let near: HashSet<EntityId> = (around.iter())
            .filter(|(_, p)| (*p - centre).length_squared() <= VIEW * VIEW)
            .map(|e| e.0)
            .collect();
        let kept: HashSet<EntityId> = around.into_iter().map(|e| e.0).collect();
        // Left: gone from the world, or beyond the view.
        let gone: Vec<EntityId> = self
            .spawned
            .iter()
            .chain(&self.pending)
            .filter(|id| !kept.contains(id))
            .copied()
            .collect();
        for id in gone {
            if self.pending.remove(&id) {
                // Never sent: unsent.
                self.outbox.retain(|q| q.spawns != Some(id));
            } else {
                self.spawned.remove(&id);
                self.queue(&ServerMessage::Despawn(id));
            }
        }
        // Came: in the order of the ids (the oldest first).
        let mut came: Vec<EntityId> = near.into_iter().filter(|id| !self.has(*id)).collect();
        came.sort_unstable();
        for id in came {
            let Some(i) = world.index(id) else { continue };
            // (A joint's side, or a load in someone's hands: not predicted on its own.)
            let joined = world.in_hands(id)
                || (world.world.joints().iter())
                    .any(|j| j.a.0 == i || j.b.is_some_and(|b| b.0 == i));
            let drawn = world.drawn(i);
            let kind = shapes.kind(id, &world.world.bodies[i], (&drawn, joined), world.tick);
            self.outbox.push_back(Queued {
                bytes: to_bytes(&ServerMessage::Spawn(id, kind)),
                spawns: Some(id),
            });
            // Its age right after it (G1.3c); unsent with it if it leaves first.
            if let Some(age) = world.age_told(id, tick) {
                self.outbox.push_back(Queued {
                    bytes: to_bytes(&ServerMessage::Age(id, age)),
                    spawns: Some(id),
                });
            }
            self.pending.insert(id);
        }
        // Edits not yet had (all of the world's: the terrain is everyone's).
        let edits = world.terrain.edits();
        let log = edits.log();
        while self.edits_told < log.len() {
            let to = (self.edits_told + EDITS_PER_MESSAGE).min(log.len());
            let mut part = Edits::default();
            for edit in &log[self.edits_told..to] {
                part.push(*edit);
            }
            let first = self.edits_told as u64;
            self.edits_told = to;
            self.queue(&ServerMessage::Edits {
                first,
                log: part.to_bytes(),
            });
        }
        // What was picked up from the ground (everyone's, as the terrain is).
        let taken = world.taken_since(self.taken_told);
        if !taken.is_empty() {
            self.taken_told += taken.len();
            self.queue(&ServerMessage::Taken(taken.to_vec()));
        }
    }

    /// Queues a structure's shape change, if the client has it.
    pub fn plastic(&mut self, id: EntityId, change: &Plastic) {
        if self.has(id) {
            self.queue(&ServerMessage::Plastic(id, change.clone()));
        }
    }

    /// Queues an entity's changed age (G1.3c), if the client has it.
    pub fn age(&mut self, id: EntityId, age: Age) {
        if self.has(id) {
            self.queue(&ServerMessage::Age(id, age));
        }
    }

    /// Sends its own character as only the server knows it (G1.2): its jetpack's gas, its
    /// health, whether it is down. A few bytes a round.
    pub fn own(&self, renet: &mut RenetServer, world: &ServerWorld) {
        let Some(avatar) = self.avatar.filter(|a| world.index(*a).is_some()) else {
            return;
        };
        let person = world.person(avatar);
        let own = to_bytes(&ServerMessage::Own {
            gas: world.gas(avatar),
            health: person.map_or(1.0, |p| p.health),
            fallen: person.is_some_and(crate::world::people::Person::fallen),
            load: world.load(avatar),
        });
        renet.send_message(self.client, UNRELIABLE, own);
    }

    /// Sends what the budget pays for this round (`seconds` since the last), then the
    /// snapshot of (network) tick `tick`.
    pub fn send(&mut self, renet: &mut RenetServer, world: &ServerWorld, seconds: f64, tick: u32) {
        self.pacer.refill(seconds);
        self.commands = (self.commands + COMMANDS_PER_SECOND * seconds).min(COMMAND_BURST);
        while let Some(q) = self.outbox.front() {
            if !self.pacer.may_send(q.bytes.len()) {
                break;
            }
            let q = self.outbox.pop_front().expect("a front");
            self.pacer.send(q.bytes.len());
            if let Some(id) = q.spawns {
                self.pending.remove(&id);
                self.spawned.insert(id);
            }
            renet.send_message(self.client, RELIABLE, q.bytes);
        }
        let centre = self.centre(world);
        let mut view: Vec<_> = self
            .spawned
            .iter()
            .filter_map(|id| {
                let body = &world.world.bodies[world.index(*id)?];
                let s = &body.structure;
                let distance = (s.position[0] - centre).length();
                Some((
                    *id,
                    state(body),
                    importance(distance, s.velocity[0].length()),
                ))
            })
            .collect();
        view.sort_by_key(|v| v.0);
        // Less the message's tag.
        let budget = self.pacer.snapshot_budget() - 1;
        // The bodies the client may predict, near its avatar (`predict::NEARBY`).
        let near: Vec<_> = (view.iter())
            .filter(|(id, ..)| {
                let at = world
                    .index(*id)
                    .map(|i| world.world.bodies[i].structure.position[0]);
                at.is_some_and(|p| (p - centre).length() < NEARBY)
            })
            .map(|v| v.0)
            .collect();
        let snapshot = (self.sender).write(tick, self.inputs.last(), &view, (budget, &near));
        let message = to_bytes(&ServerMessage::Snapshot(snapshot));
        self.pacer.snapshot(message.len());
        self.pacer.flush();
        renet.send_message(self.client, UNRELIABLE, message);
    }
}
