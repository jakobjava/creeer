//! What the player's hands hold, steered (G2.10): the hold sent with each input from now on,
//! and the thing carried as the server last told it.

use super::Connection;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::{Carried, EntityId, Kind};

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
        self.own_carried().map(|c| c.load)
    }
}

/// The view `view`'s heading, upright along `up`: the view turned level about its own right
/// axis. What the hands hold turns with it, not as the eyes look up or down (G2.10: taken
/// looking down, a lying stick stood upright once the view came level).
pub fn heading(view: DQuat, up: DVec3) -> DQuat {
    let forward = view * DVec3::NEG_Z;
    match forward.reject_from(up).try_normalize() {
        Some(level) => (DQuat::from_rotation_arc(forward, level) * view).normalize(),
        None => view,
    }
}

impl Connection {
    /// What the player's own avatar carries as the server last told it: the load, the point
    /// held in the frame it is drawn in, one hand or two.
    pub fn own_carried(&self) -> Option<Carried> {
        let own = self.welcome.as_ref()?.avatar;
        match self.kinds.get(&own) {
            Some(Kind::Character { carries, .. }) => *carries,
            _ => None,
        }
    }
}
