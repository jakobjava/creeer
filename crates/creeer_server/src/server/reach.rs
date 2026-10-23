//! What the players reach for and strike, judged as they saw it (G1.4d–e, `world/reach.rs`,
//! `world/strikes.rs`): a client counts the server's ticks as its snapshots are numbered, the
//! world its own (which go back when a save is loaded); and what they carry (G1.6a).

use super::Server;
use creeer_net::entity::EntityId;
use creeer_net::message::{Command, ServerMessage, Target};

impl Server {
    /// The world's tick at server tick `net` (as snapshots are numbered; fractional).
    pub fn world_tick(&self, net: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)] // ticks within 2⁵³
        let world = self.world.tick as f64;
        net - f64::from(self.net_tick) + world
    }

    /// Avatar `avatar`'s player's `command`, a strike's ticks turned to the world's.
    pub(super) fn command(&mut self, command: &Command, avatar: EntityId) -> Result<(), String> {
        let seen = |t: &Target| Target {
            seen: self.world_tick(t.seen),
            ..*t
        };
        let command = match command {
            Command::Strike { target, started } => Command::Strike {
                target: seen(target),
                started: self.world_tick(*started),
            },
            Command::Grab(t) => Command::Grab(seen(t)),
            Command::Stow(Some(t)) => Command::Stow(Some(seen(t))),
            Command::PutIn(t) => Command::PutIn(seen(t)),
            Command::TakeFrom { container, item } => Command::TakeFrom {
                container: seen(container),
                item: *item,
            },
            Command::Open(t) => Command::Open(seen(t)),
            Command::Bind(t) => Command::Bind(seen(t)),
            Command::Fasten { target, fastener } => Command::Fasten {
                target: seen(target),
                fastener: fastener.clone(),
            },
            Command::StrikeLoose { loose, started } => Command::StrikeLoose {
                loose: *loose,
                started: self.world_tick(*started),
            },
            Command::StrikeTree {
                tree,
                height,
                started,
            } => Command::StrikeTree {
                tree: *tree,
                height: *height,
                started: self.world_tick(*started),
            },
            other => other.clone(),
        };
        self.world.apply(&command, Some(avatar))
    }

    /// Client `id`'s player's command was refused, and why: told to them (G1.6), and noted.
    pub(super) fn refuse(&mut self, id: creeer_net::renet::ClientId, player: String, why: String) {
        if let Some(s) = self.sessions.get_mut(&id) {
            s.queue(&ServerMessage::Refusal(why.clone()));
        }
        self.events.push(super::Event::CommandRefused(player, why));
    }

    /// Tells each player whose inventory changed what they now carry (G1.6a).
    pub(super) fn tell_inventories(&mut self) {
        for (avatar, note) in self.world.take_notes() {
            let note = ServerMessage::Note(note);
            for s in self
                .sessions
                .values_mut()
                .filter(|s| s.avatar == Some(avatar))
            {
                s.queue(&note);
            }
        }
        for avatar in self.world.take_inventories() {
            let inventory = ServerMessage::Inventory(self.world.inventory(avatar));
            for s in self
                .sessions
                .values_mut()
                .filter(|s| s.avatar == Some(avatar))
            {
                s.queue(&inventory);
            }
        }
    }

    /// Tells the sessions that see them of the swings started since, as of network ticks.
    pub(super) fn tell_swings(&mut self) {
        for (id, started) in self.world.take_swings_started() {
            // (Begun a few ticks ahead, or a tick or two back.)
            let ahead = i64::try_from(started).unwrap_or(i64::MAX)
                - i64::try_from(self.world.tick).unwrap_or(i64::MAX);
            let at = i64::from(self.net_tick) + ahead;
            let Ok(at) = u32::try_from(at) else { continue };
            for session in self.sessions.values_mut().filter(|s| s.has(id)) {
                session.queue(&ServerMessage::Swing(id, at));
            }
        }
    }
}

#[cfg(test)]
#[path = "items_tests.rs"]
mod items_tests;
