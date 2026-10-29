//! What the player's hands hold, steered (G2.10): the hold sent with each input from now on,
//! and the thing carried as the server last told it.

use super::Connection;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::{Carried, EntityId, Kind};
use creeer_net::message::Hold;
use creeer_sim::reach;

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

    /// What the hands hold as it is drawn now: where it is from the avatar's eyes in the view
    /// `view`'s frame, and its turn from the view's heading. Steering starts there
    /// (`creeer_client`'s `holding.rs`). Measured from the eyes, not the camera: the camera
    /// follows the avatar 4.5 m behind, and a hold kept from it swung round the camera as the
    /// view turned (G2.10, the user: "things float away").
    pub fn held_in_view(&self, view: DQuat) -> Option<(DVec3, DQuat)> {
        let carried = self.own_carried()?;
        let avatar = self.avatar()?;
        let pose = self.poses().get(&carried.load).copied()?;
        let up = avatar.position.normalize();
        let eye = reach::eye(avatar.position, up);
        Some((
            view.conjugate() * (pose.position - eye),
            heading(view, up).conjugate() * pose.orientation,
        ))
    }

    /// The hold for what the hands hold meant at `at` from the avatar's eyes in the view
    /// `view`'s frame, turned `turn` from its heading (`steady`: held still in the world). It is
    /// brought within the arms' reach at its grip as the server's hands hold it
    /// (`reach::within_arms`), so it is never meant where no hand reaches. Returns the hold, and
    /// `at` as reachable.
    pub fn hold_in_view(
        &self,
        view: DQuat,
        (at, turn): (DVec3, DQuat),
        steady: bool,
    ) -> Option<(Hold, DVec3)> {
        let carried = self.own_carried()?;
        let avatar = self.avatar()?;
        let up = avatar.position.normalize();
        let eye = reach::eye(avatar.position, up);
        let turn = (heading(view, up) * turn).normalize();
        let grip = turn * carried.grip;
        let held = reach::within_arms(avatar.position, up, eye + view * at + grip);
        let node = held - grip;
        let hold = Hold {
            offset: node - avatar.position,
            turn,
            steady,
        };
        Some((hold, view.conjugate() * (node - eye)))
    }
}
