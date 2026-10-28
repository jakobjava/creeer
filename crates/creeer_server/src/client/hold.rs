//! What the player's hands hold, steered (G2.10): the hold sent with each input from now on,
//! and the thing carried as the server last told it.

use super::Connection;
use creeer_net::entity::{EntityId, Kind};

impl Connection {
    /// Steers what the hands hold to `hold` from the next input on (the client's
    /// `holding.rs`; none: as they hold it by themselves).
    pub fn set_hold(&mut self, hold: Option<creeer_net::message::Hold>) {
        if let Some(p) = &mut self.predicted {
            p.set_hold(hold);
        }
    }

    /// The thing the player's own avatar carries, if any (as the server last told it).
    pub fn own_load(&self) -> Option<EntityId> {
        let own = self.welcome.as_ref()?.avatar;
        match self.kinds.get(&own) {
            Some(Kind::Character { carries, .. }) => carries.map(|c| c.load),
            _ => None,
        }
    }
}
