//! Cameras placed on a planet: level with the horizon, looking a given way; and the player's
//! own (G2.11, A4; ADR-0042): first person at the eyes (the head's when down), third person on a
//! boom behind them that is pulled in before anything between.

use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_render::Camera;
use creeer_sim::reach;

/// A camera at `position` looking along `forward`, level with the local horizon.
pub fn place(position: DVec3, forward: DVec3) -> Camera {
    place_up(position, forward, position.normalize())
}

/// A camera at `position` looking along `forward`, level with the horizon of `up` (for
/// frames not centred on a planet).
pub fn place_up(position: DVec3, forward: DVec3, up: DVec3) -> Camera {
    let mut camera = Camera::new(position);
    camera.level_to(up);
    // Yaw and pitch of `forward` in the horizon frame.
    let local = camera.frame.inverse() * forward.normalize();
    camera.yaw = det::atan2(-local.x, -local.z);
    camera.pitch = det::asin(local.y.clamp(-0.995, 0.995));
    camera
}

/// Where the third-person camera wants to be: this far behind the avatar's centre along the
/// view, and this far above it (m).
const BEHIND: f64 = 4.5;
const ABOVE: f64 = 1.2;
/// How near anything the boom lets the camera come (m): its near plane (5 cm) with room.
const MARGIN: f64 = 0.2;
/// How close behind the eyes the camera may come before the player's own body is hidden (m):
/// nearer, it would look out from inside the head.
const OWN_BODY: f64 = 0.7;
/// How fast the boom lets the camera back out once nothing is in the way (s, its time
/// constant): pulled in at once, let out gently (Cinemachine's collider damps the way back).
const BACK_OUT: f64 = 0.3;

/// The first-person eye of an avatar whose body's centre is at `centre`, upright along `up`.
/// (Not lifted off the ground, as the free camera is: the eyes are where the head is, in a
/// hole or on a mound's flank alike. Lifted 1.5 m clear of the ground under them, as before, they
/// rose wherever the ground's density read less than the height, near edits and when down.)
pub fn eye(centre: DVec3, up: DVec3) -> DVec3 {
    reach::eye(centre, up)
}

/// Where the player's own head is while they are down: their ragdoll's head segment, as drawn.
pub fn own_head(c: &creeer_server::Connection) -> Option<DVec3> {
    use creeer_net::entity::Kind;
    /// The head's segment in the ragdoll (`bones::SEGMENT_NAMES`).
    const HEAD: u8 = 2;
    let own = c.welcome.as_ref()?.avatar;
    let poses = c.poses();
    (c.kinds.iter()).find_map(|(id, kind)| match kind {
        Kind::Segment { person, index, .. } if *person == own && *index == HEAD => {
            poses.get(id).map(|p| p.position)
        }
        _ => None,
    })
}

/// The third-person camera's boom: how far out it is, kept from frame to frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Boom {
    out: Option<f64>,
}

impl Boom {
    /// The third-person camera of an avatar whose body's centre is at `centre` (upright along
    /// `up`), looked from along `forward`, `dt` seconds after the last: on a boom from its eyes
    /// to where it wants to be ([`BEHIND`], [`ABOVE`]), pulled in at once before what `blocked`
    /// finds between (the distance to it along a ray from a point in a direction, within a
    /// length; five rays, the boom's and four a margin round it, so the near plane's corners stay
    /// clear), let out again over [`BACK_OUT`]. Returns the camera's place, and whether it is so
    /// near the eyes that the player's own body must not be drawn.
    pub fn place(
        &mut self,
        (centre, up, forward): (DVec3, DVec3, DVec3),
        dt: f64,
        blocked: impl Fn(DVec3, DVec3, f64) -> Option<f64>,
    ) -> (DVec3, bool) {
        let pivot = eye(centre, up);
        let wanted = centre + up * ABOVE - forward * BEHIND;
        let off = wanted - pivot;
        let length = off.length();
        let way = off / length;
        let (a, b) = way.any_orthonormal_pair();
        let clear = [DVec3::ZERO, a, -a, b, -b]
            .iter()
            .filter_map(|o| blocked(pivot + *o * MARGIN, way, length + MARGIN))
            .fold(f64::INFINITY, f64::min);
        let free = (clear - MARGIN).clamp(0.0, length);
        let out = match self.out {
            Some(was) if was <= free => was + (free - was) * (1.0 - det::exp(-dt / BACK_OUT)),
            _ => free,
        };
        self.out = Some(out);
        (pivot + way * out, out < OWN_BODY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UP: DVec3 = DVec3::Y;

    /// In the open the boom is out its whole length behind and above the avatar.
    #[test]
    fn in_the_open_the_camera_is_behind_and_above() {
        let mut boom = Boom::default();
        let (at, hide) = boom.place((DVec3::ZERO, UP, DVec3::NEG_Z), 0.016, |_, _, _| None);
        assert!(
            (at - DVec3::new(0.0, ABOVE, BEHIND)).length() < 1e-9,
            "{at}"
        );
        assert!(!hide);
    }

    /// A wall 2 m behind: the camera is pulled in before it at once, the near plane clear of it;
    /// the wall gone, it backs out over a few tenths of a second, never jumping.
    #[test]
    fn a_wall_behind_pulls_the_camera_in_at_once_and_it_backs_out_gently() {
        let mut boom = Boom::default();
        let wall = |from: DVec3, way: DVec3, most: f64| {
            // A plane z = 2 behind the avatar.
            let t = (2.0 - from.z) / way.z;
            (way.z > 0.0 && t <= most).then_some(t)
        };
        let view = (DVec3::ZERO, UP, DVec3::NEG_Z);
        let (at, _) = boom.place(view, 0.016, |_, _, _| None);
        assert!(at.z > 4.0);
        let (at, _) = boom.place(view, 0.016, wall);
        assert!(at.z < 2.0 - 0.1 && at.z > 1.0, "{at}");
        let mut last = at;
        let mut frames = 0;
        while (last.z - BEHIND).abs() > 0.01 && frames < 600 {
            let (next, _) = boom.place(view, 1.0 / 144.0, |_, _, _| None);
            assert!(
                (next - last).length() < 0.15,
                "a jump of {}",
                (next - last).length()
            );
            last = next;
            frames += 1;
        }
        assert!(frames < 288, "backed out in {frames} frames");
    }

    /// Against something at the eyes' back the camera comes to the eyes, and the own body is not
    /// drawn (the camera would be inside it).
    #[test]
    fn blocked_at_the_eyes_the_own_body_is_hidden() {
        let mut boom = Boom::default();
        let (at, hide) = boom.place((DVec3::ZERO, UP, DVec3::NEG_Z), 0.016, |_, _, _| Some(0.1));
        assert!((at - eye(DVec3::ZERO, UP)).length() < 1e-9);
        assert!(hide);
    }
}
