//! What changes the world a client draws, held until it draws the tick it changed at
//! (2026-10-28, the user: "when you take the branches off a tree the tree flashes").
//!
//! A client draws the world [`super::DELAY`] ticks in the past, between snapshots
//! (`creeer_net::interp`): a body is drawn from the first snapshot that has it to the last.
//! What comes reliably came at once: a felled part limbed was despawned as soon as the message
//! came, while its successor, spawned in the same tick, was drawn only once the past drawn
//! reached its first snapshot, so for 3 ticks neither was. The same held for a tree going over
//! (its stem cut at the stump before its trunk was drawn), a stone struck (taken from the
//! ground before its body was drawn) and anything laid at rest (laid while its body was still
//! drawn). Now each round's reliable messages follow a [`ServerMessage::Round`] naming the
//! snapshot they went with, and what changes the drawn world (a despawn, things laid or taken,
//! trees cut) is held until the snapshot whose bodies are drawn is that round's: the old and
//! the new change places within one frame. Unity's Netcode for Entities does the same for its
//! interpolated ghosts: a despawn waits until the interpolation tick reaches the tick it was
//! despawned at, and a spawn until it reaches the tick it came at (`GhostDespawnSystem`,
//! "Ghost spawning", Netcode for Entities manual).

use super::Connection;
use creeer_net::entity::EntityId;
use creeer_net::message::ServerMessage;
use std::collections::VecDeque;

/// The messages held, each with the round it went with.
#[derive(Debug, Default)]
pub(crate) struct Held {
    /// The round the reliable messages now coming went with (none before the first).
    round: Option<u32>,
    messages: VecDeque<(u32, ServerMessage)>,
}

impl Connection {
    /// The reliable messages after this went with the snapshot of network tick `tick`.
    pub(super) fn round(&mut self, tick: u32) {
        self.held.round = Some(tick);
    }

    /// `message` changes what is drawn: held until its round is drawn (at once before any
    /// round was told).
    pub(super) fn hold(&mut self, message: ServerMessage) {
        match self.held.round {
            Some(round) => self.held.messages.push_back((round, message)),
            None => self.apply(message),
        }
    }

    /// The held messages of the rounds drawn now, applied in the order they came.
    pub(super) fn apply_drawn(&mut self) {
        let drawn = self.drawn_round();
        while self.held.messages.front().is_some_and(|m| m.0 <= drawn) {
            if let Some((_, message)) = self.held.messages.pop_front() {
                self.apply(message);
            }
        }
    }

    /// The round whose bodies are drawn now: the first snapshot after the tick drawn at (the
    /// newest, drawn run on past it; any, before the first snapshot).
    fn drawn_round(&self) -> u32 {
        let mut snapshots = self.receiver.snapshots().peekable();
        if snapshots.peek().is_none() {
            return u32::MAX;
        }
        let mut last = 0;
        for s in snapshots {
            last = s.tick;
            if f64::from(s.tick) > self.drawn_at {
                break;
            }
        }
        last
    }

    /// A despawn of `id` held, done now: it is spawned again as something else (a character
    /// fallen or up again), replacing what was drawn at once, as before.
    pub(super) fn despawn_now(&mut self, id: EntityId) {
        let held = std::mem::take(&mut self.held.messages);
        for (round, message) in held {
            match message {
                ServerMessage::Despawn(gone) if gone == id => self.apply(message),
                other => self.held.messages.push_back((round, other)),
            }
        }
    }

    /// What `message` changes, now.
    fn apply(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Despawn(id) => {
                self.aim_shapes.forget(id);
                self.kinds.remove(&id);
                self.broken.remove(&id);
                self.ages.remove(&id);
                self.swings.remove(&id);
                self.items.remove(&id);
            }
            ServerMessage::Taken(ids) => {
                for id in &ids {
                    self.laid.remove(id);
                }
                self.taken.extend(ids);
            }
            ServerMessage::Laid(laid) => {
                for l in laid {
                    self.laid.insert(l.id, l);
                }
            }
            ServerMessage::Trees(cuts) => {
                for c in cuts {
                    self.trees.insert(c.id, c);
                }
            }
            _ => {}
        }
    }
}
