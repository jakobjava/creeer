//! Steering what the hands hold (G2.10, ADR-0036): held where the player means it, as people
//! hold things before them. Taken, it is held where it was, from the avatar's eyes (not the
//! camera behind them) in the view's frame; it moves with the view within the arms' reach,
//! turning with the view's heading only; the wheel pushes it away or draws it in; Q with the
//! mouse turns it (left–right and up–down about the heading), Q with the wheel rolls it; Ctrl
//! makes all of it five times finer;
//! H holds it still where it is (steady), the view and the other hand free to fasten it (J).
//! Sent with each input (`creeer_net::message::Hold`): the server's hands bring it there as
//! fast and as firmly as their strength allows (`creeer_server`'s `hands.rs`).

use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::EntityId;

use creeer_render::Camera;
use creeer_server::Connection;
use winit::keyboard::KeyCode;

/// How far a notch of the wheel moves it (m), and how far from the eyes it is held (m: at most
/// to the ground before the feet; the arms' reach holds it nearer, `reach::within_arms`).
const NOTCH: f64 = 0.02;
const NEAREST: f64 = 0.25;
const FARTHEST: f64 = 1.9;
/// How far the mouse turns it (rad per pixel), and a notch rolls it (rad).
const PER_PIXEL: f64 = 0.006;
const ROLL: f64 = 0.08;
/// Fine: everything this much slower.
const FINE: f64 = 0.2;

/// The player's steering of what their hands hold.
#[derive(Debug, Default)]
pub struct Holding {
    /// The thing held, as the server told it.
    load: Option<EntityId>,
    /// Where its centre is held, in the view's frame (x right, y up, −z ahead), and its turn.
    at: DVec3,
    turn: DQuat,
    /// Held still where it is (H).
    pub steady: bool,
    /// Q held: the mouse and the wheel turn it. Ctrl held: all of it finer.
    turning: bool,
    pub fine: bool,
    /// Whether it was held steady last frame.
    was_steady: bool,
}

impl Holding {
    /// Whether anything is held.
    pub fn holding(&self) -> bool {
        self.load.is_some()
    }

    /// A key (Q, H, Ctrl); whether it was one for what is held. Q and Ctrl are followed with
    /// nothing held too: let go of while Q was down, the next thing taken turned with the mouse.
    pub fn key(&mut self, code: KeyCode, pressed: bool) -> bool {
        match code {
            KeyCode::KeyQ => self.turning = pressed,
            KeyCode::ControlLeft | KeyCode::ControlRight => self.fine = pressed,
            KeyCode::KeyH if pressed && self.holding() => self.steady = !self.steady,
            KeyCode::KeyH => {}
            _ => return false,
        }
        self.holding()
    }

    /// A turn of the wheel while something is held: pushed away or drawn in, or rolled with Q;
    /// whether it took it.
    pub fn wheel(&mut self, notches: f64) -> bool {
        if !self.holding() || self.steady {
            return false;
        }
        let gain = if self.fine { FINE } else { 1.0 };
        if self.turning {
            self.turn = DQuat::from_rotation_z(-notches * ROLL * gain) * self.turn;
        } else {
            let distance = (self.at.length() + notches * NOTCH * gain).clamp(NEAREST, FARTHEST);
            self.at = self.at.normalize_or(DVec3::NEG_Z) * distance;
        }
        true
    }

    /// The mouse moved: with Q held it turns what is held (and not the view). Whether it took
    /// it, and how much the view turns with the mouse (Ctrl held: finer, for placing it).
    pub fn mouse(&mut self, (dx, dy): (f64, f64)) -> (bool, f64) {
        let fine = if self.holding() && self.fine {
            FINE
        } else {
            1.0
        };
        if !(self.holding() && self.turning) || self.steady {
            return (false, fine);
        }
        let gain = PER_PIXEL * fine;
        let turn = DQuat::from_rotation_y(dx * gain) * DQuat::from_rotation_x(dy * gain);
        self.turn = (turn * self.turn).normalize();
        (true, fine)
    }

    /// Each frame: what is held now (taken: held where it is, from the avatar's eyes in the
    /// view `camera`'s frame), and the hold sent with the inputs from now on, within the arms'
    /// reach (`Connection::hold_in_view`).
    pub fn update(&mut self, camera: &Camera, connection: &mut Connection) {
        let view = camera.rotation();
        let load = connection.own_load();
        if load != self.load {
            self.load = load;
            self.steady = false;
            self.take(view, connection);
        }
        // Let go of steady: held from where it is now (it does not jump back).
        if self.load.is_some() && !self.steady && self.was_steady {
            self.take(view, connection);
        }
        self.was_steady = self.steady;
        let held = self
            .load
            .and_then(|_| connection.hold_in_view(view, (self.at, self.turn), self.steady));
        if let Some((_, at)) = held
            && !self.steady
        {
            // Meant beyond the arms' reach, it is held at their reach: the wheel and the view
            // go on from there.
            self.at = at;
        }
        connection.set_hold(held.map(|h| h.0));
    }

    /// Held from where it is drawn now, from the eyes in the view `view`'s frame.
    fn take(&mut self, view: DQuat, connection: &Connection) {
        (self.at, self.turn) = connection
            .held_in_view(view)
            .unwrap_or((DVec3::new(0.0, -0.5, -0.6), DQuat::IDENTITY));
    }

    /// The keys' hint while something is held.
    pub fn hint(&self) -> Option<&'static str> {
        self.holding().then_some(if self.steady {
            "held steady · J fasten · K fastener · H let it move · G put down"
        } else {
            "wheel nearer/farther · Q+mouse turn · Q+wheel roll · Ctrl fine · H steady · G put down · T throw"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Holding something at the default place before the eyes.
    fn held() -> Holding {
        Holding {
            load: Some(EntityId(5)),
            at: DVec3::new(0.0, -0.3, -0.6),
            ..Holding::default()
        }
    }

    #[test]
    fn nothing_held_takes_no_keys_wheel_or_mouse() {
        let mut h = Holding::default();
        assert!(!h.key(KeyCode::KeyQ, true) && !h.key(KeyCode::KeyH, true));
        assert!(!h.wheel(1.0));
        assert_eq!(h.mouse((10.0, 0.0)), (false, 1.0));
        assert!(h.hint().is_none() && !h.steady);
    }

    #[test]
    fn the_wheel_moves_it_two_centimetres_a_notch_within_reach_and_finer_with_ctrl() {
        let mut h = held();
        let from = h.at.length();
        assert!(h.wheel(1.0));
        assert!((h.at.length() - from - NOTCH).abs() < 1e-12);
        assert!(h.key(KeyCode::ControlLeft, true));
        h.wheel(1.0);
        assert!((h.at.length() - from - NOTCH * (1.0 + FINE)).abs() < 1e-12);
        assert_eq!(h.mouse((0.0, 0.0)), (false, FINE), "the view finer too");
        for _ in 0..1000 {
            h.wheel(1.0);
        }
        assert!((h.at.length() - FARTHEST).abs() < 1e-12);
        for _ in 0..1000 {
            h.wheel(-1.0);
        }
        assert!((h.at.length() - NEAREST).abs() < 1e-12);
    }

    #[test]
    fn with_q_the_mouse_turns_it_and_the_wheel_rolls_it() {
        let mut h = held();
        let at = h.at;
        assert!(h.key(KeyCode::KeyQ, true));
        assert_eq!(h.mouse((100.0, 0.0)), (true, 1.0), "not the view");
        let yaw = h.turn.angle_between(DQuat::IDENTITY);
        assert!((yaw - 100.0 * PER_PIXEL).abs() < 1e-9, "{yaw}");
        h.turn = DQuat::IDENTITY;
        h.wheel(2.0);
        assert!((h.turn.angle_between(DQuat::IDENTITY) - 2.0 * ROLL).abs() < 1e-9);
        assert_eq!(h.at, at, "rolled, not moved");
    }

    #[test]
    fn held_steady_neither_the_wheel_nor_the_mouse_moves_it() {
        let mut h = held();
        assert!(h.key(KeyCode::KeyH, true) && h.steady);
        assert!(h.key(KeyCode::KeyH, false) && h.steady, "a press toggles");
        h.key(KeyCode::KeyQ, true);
        assert!(!h.wheel(1.0) && !h.mouse((5.0, 5.0)).0);
        assert!(h.hint().is_some_and(|t| t.contains("J fasten")));
        h.key(KeyCode::KeyH, true);
        assert!(!h.steady);
    }

    #[test]
    fn q_let_go_of_with_nothing_held_does_not_turn_the_next_thing_taken() {
        let mut h = held();
        h.key(KeyCode::KeyQ, true);
        h.load = None;
        assert!(!h.key(KeyCode::KeyQ, false), "passed on: nothing held");
        h.load = Some(EntityId(6));
        assert_eq!(h.mouse((10.0, 0.0)), (false, 1.0), "the view turns");
    }
}
