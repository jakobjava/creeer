//! What the players reach for and strike, judged as they saw it (G1.4d–e, `world/reach.rs`,
//! `world/strikes.rs`): a client counts the server's ticks as its snapshots are numbered, the
//! world its own (which go back when a save is loaded).

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
        if let Command::Strike { target, started } = command {
            let target = Target {
                seen: self.world_tick(target.seen),
                ..*target
            };
            let started = self.world_tick(*started);
            return (self.world).apply(&Command::Strike { target, started }, Some(avatar));
        }
        if let Command::Grab(target) = command {
            let seen = self.world_tick(target.seen);
            return (self.world).apply(&Command::Grab(Target { seen, ..*target }), Some(avatar));
        }
        self.world.apply(command, Some(avatar))
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
