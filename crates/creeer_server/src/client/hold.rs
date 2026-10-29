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

/// How far the hands go up or down as the view looks up or down (m per unit of the sine of the
/// view's pitch: looking 45° down lays it on the ground before the feet, as one looks there to
/// place it), at most down (to the ground, stooping) and up (above the
/// head); and how far the wheel holds it out beyond the chest at most (m).
const LOOKED: f64 = 1.6;
const LOWEST: f64 = 1.35;
const HIGHEST: f64 = 0.5;
const MOST_OUT: f64 = 0.45;
/// How far to the right of the body's middle one hand carries (m).
const ONE_HAND_ASIDE: f64 = 0.12;
/// How far before the body's axis a thing placed is held at least (m).
const IN_FRONT: f64 = 0.3;

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
        // Placed before the body, never beside or behind it: a person turns to what they place.
        let forward = heading(view, up) * DVec3::NEG_Z;
        let meant = eye + view * at + grip;
        let ahead = (meant - avatar.position).dot(forward);
        let meant = meant + forward * (IN_FRONT - ahead).max(0.0);
        let held = reach::within_arms(avatar.position, up, meant);
        let node = held - grip;
        let hold = Hold {
            offset: node - avatar.position,
            turn,
            steady,
        };
        Some((hold, view.conjugate() * (node - eye)))
    }
}

impl Connection {
    /// The hold for what the hands hold: before the chest the way the view `view` heads, `out`
    /// m farther out (the wheel), lowered as the view looks down and raised as it looks up (to
    /// lay it on what lies low, or lift it onto what stands high), turned `turn` from that
    /// heading. It goes with the body; within the arms' reach the body stoops to it (G2.10,
    /// the user's play: one way to hold and place, the body leading).
    pub fn carry_hold(&self, view: DQuat, turn: DQuat, out: f64) -> Option<Hold> {
        let carried = self.own_carried()?;
        let avatar = self.avatar()?;
        let up = avatar.position.normalize();
        let heading = heading(view, up);
        let forward = heading * DVec3::NEG_Z;
        // Before the chest, as the server's hands carry by themselves (`carry::held`); in one
        // hand, the right, before its own half of the chest (at the middle, the forearm
        // crossed the body).
        let aside = if carried.one_hand {
            ONE_HAND_ASIDE
        } else {
            0.0
        };
        let looked = (view * DVec3::NEG_Z).dot(up).clamp(-1.0, 1.0);
        let chest = creeer_sim::carry::held(avatar.position, up, forward)
            + (heading * DVec3::X) * aside
            + forward * out.clamp(0.0, MOST_OUT)
            + up * (looked * LOOKED).clamp(-LOWEST, HIGHEST);
        let turn = (heading * turn).normalize();
        let grip = reach::within_arms(avatar.position, up, chest);
        Some(Hold {
            offset: grip - turn * carried.grip - avatar.position,
            turn,
            steady: false,
        })
    }
}
