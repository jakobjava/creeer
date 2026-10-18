//! What the server tells a client as it plays: its welcome, what comes into and leaves its
//! view, the snapshots, and its own character's state only the server knows.

use super::{Connection, Status, Welcome};
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
            }
            ServerMessage::Edits { first, log } => self.edited(first, &log),
            ServerMessage::Plastic(id, plastic) => self.deformed(id, &plastic),
            ServerMessage::Snapshot(snapshot) => self.snapshot(&snapshot),
            ServerMessage::Machines(list) => self.machines = list,
            ServerMessage::Profile(report) => self.profile = Some(report),
            ServerMessage::Own {
                gas,
                health,
                fallen,
            } => {
                (self.gas, self.health, self.fallen) = (gas, health, fallen);
                if let Some(p) = &mut self.predicted {
                    p.set_gas(gas);
                    p.set_fallen(fallen);
                }
            }
        }
    }

    pub(super) fn snapshot(&mut self, bytes: &[u8]) {
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
}
