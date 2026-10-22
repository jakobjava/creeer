//! What the server tells a client as it plays: its welcome, what comes into and leaves its
//! view, the snapshots, and its own character's state only the server knows.

use super::{Connection, Status, Welcome};
use creeer_net::channel::TICK_RATE;
use creeer_net::interp::sample;
use creeer_net::message::ServerMessage;

impl Connection {
    pub(super) fn handle(&mut self, message: ServerMessage) {
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
                self.ages.remove(&id);
                self.swings.remove(&id);
            }
            ServerMessage::Age(id, age) => {
                self.ages.insert(id, age);
            }
            // (The player's own swing is its own, from when it asked.)
            ServerMessage::Swing(id, at) => {
                if self.welcome.as_ref().is_none_or(|w| w.avatar != id) {
                    self.swings.insert(id, at);
                }
            }
            ServerMessage::Edits { first, log } => self.edited(first, &log),
            ServerMessage::Plastic(id, plastic) => self.deformed(id, &plastic),
            ServerMessage::Snapshot(snapshot) => self.snapshot(&snapshot),
            ServerMessage::Machines(list) => self.machines = list,
            ServerMessage::Profile(report) => self.profile = Some(report),
            ServerMessage::Inventory(inventory) => self.inventory = inventory,
            ServerMessage::Own {
                gas,
                health,
                fallen,
                load,
            } => {
                (self.gas, self.health, self.fallen) = (gas, health, fallen);
                if let Some(p) = &mut self.predicted {
                    p.set_gas(gas);
                    p.set_load(load);
                    p.set_fallen(fallen);
                }
            }
        }
    }

    pub(super) fn snapshot(&mut self, bytes: &[u8]) {
        // Where each entity is drawn before this snapshot (a body coming into the prediction
        // slides from there).
        let drawn = sample(&self.receiver, self.drawn_at, 1.0 / f64::from(TICK_RATE));
        let Ok(snapshot) = self.receiver.read(bytes) else {
            return;
        };
        if let (Some(w), Some(p)) = (&self.welcome, &mut self.predicted) {
            let at = |id| drawn.get(&id).map(|pose| pose.position);
            p.reconcile(snapshot, (w.avatar, &self.kinds), at);
        }
    }
}
