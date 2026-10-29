//! What the hands hold (G2.10, ADR-0036), one way (the user, 2026-10-29: one simple way to hold
//! and place, the body leading): held before the chest, going with the body the way the view
//! heads; lowered as the view looks down and raised as it looks up, to lay it on what lies low
//! or lift it onto what stands high; the wheel holds it farther out or nearer; Q with the mouse
//! turns it, Q with the wheel rolls it; Ctrl makes all of it five times finer. J fastens it
//! where it touches: the hands keep it still meanwhile (`Connection::carry_hold`).

use creeer_math::glam::DQuat;
use creeer_net::entity::EntityId;
use creeer_render::Camera;
use creeer_server::Connection;
use winit::keyboard::KeyCode;

/// How far a notch of the wheel holds it out (m).
const NOTCH: f64 = 0.03;
/// How far the mouse turns it (rad per pixel), and a notch rolls it (rad).
const PER_PIXEL: f64 = 0.006;
const ROLL: f64 = 0.08;
/// Fine: everything this much slower.
const FINE: f64 = 0.2;

/// The player's handling of what their hands hold.
#[derive(Debug, Default)]
pub struct Holding {
    /// The thing held, as the server told it.
    load: Option<EntityId>,
    /// How far beyond the chest it is held out (m), and its turn from the view's heading.
    out: f64,
    turn: DQuat,
    /// Q held: the mouse and the wheel turn it. Ctrl held: finer.
    turning: bool,
    pub fine: bool,
}

impl Holding {
    /// Whether anything is held.
    pub fn holding(&self) -> bool {
        self.load.is_some()
    }

    /// A key (Q, Ctrl); whether it was one for what is held. Followed with nothing held too:
    /// let go of while Q was down, the next thing taken turned with the mouse.
    pub fn key(&mut self, code: KeyCode, pressed: bool) -> bool {
        match code {
            KeyCode::KeyQ => self.turning = pressed,
            KeyCode::ControlLeft | KeyCode::ControlRight => self.fine = pressed,
            _ => return false,
        }
        self.holding()
    }

    /// A turn of the wheel while something is held: rolled with Q, else held out farther or
    /// nearer. Whether it took it.
    pub fn wheel(&mut self, notches: f64) -> bool {
        if !self.holding() {
            return false;
        }
        let gain = if self.fine { FINE } else { 1.0 };
        if self.turning {
            self.turn = DQuat::from_rotation_z(-notches * ROLL * gain) * self.turn;
        } else {
            self.out = (self.out + notches * NOTCH * gain).clamp(0.0, 0.45);
        }
        true
    }

    /// The mouse moved: with Q held it turns what is held (and not the view). Whether it took
    /// it, and how much the view turns with the mouse (Ctrl held: finer).
    pub fn mouse(&mut self, (dx, dy): (f64, f64)) -> (bool, f64) {
        let fine = if self.holding() && self.fine {
            FINE
        } else {
            1.0
        };
        if !(self.holding() && self.turning) {
            return (false, fine);
        }
        let gain = PER_PIXEL * fine;
        let turn = DQuat::from_rotation_y(dx * gain) * DQuat::from_rotation_x(dy * gain);
        self.turn = (turn * self.turn).normalize();
        (true, fine)
    }

    /// Each frame: what is held now (taken: its turn from the view's heading as it lies, held
    /// at the chest), and the hold sent with the inputs from now on.
    pub fn update(&mut self, camera: &Camera, connection: &mut Connection) {
        let view = camera.rotation();
        let load = connection.own_load();
        if load != self.load {
            self.load = load;
            self.out = 0.0;
            self.turn = (connection.held_in_view(view)).map_or(DQuat::IDENTITY, |h| h.1);
        }
        let hold = self
            .load
            .and_then(|_| connection.carry_hold(view, self.turn, self.out));
        connection.set_hold(hold);
    }

    /// The keys' hint while something is held.
    pub fn hint(&self) -> Option<&'static str> {
        self.holding().then_some(
            "look down/up: lower/raise · wheel: out/in · Q+mouse: turn · J: fasten where it touches · G: put down · T: throw",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held() -> Holding {
        Holding {
            load: Some(EntityId(5)),
            ..Holding::default()
        }
    }

    #[test]
    fn nothing_held_takes_no_keys_wheel_or_mouse() {
        let mut h = Holding::default();
        assert!(!h.key(KeyCode::KeyQ, true) && !h.wheel(1.0));
        assert_eq!(h.mouse((10.0, 0.0)), (false, 1.0));
        assert!(h.hint().is_none());
    }

    #[test]
    fn the_wheel_holds_it_out_a_notch_at_a_time_finer_with_ctrl_and_within_reach() {
        let mut h = held();
        assert!(h.wheel(1.0));
        assert!((h.out - NOTCH).abs() < 1e-12);
        h.key(KeyCode::ControlLeft, true);
        h.wheel(1.0);
        assert!((h.out - NOTCH * (1.0 + FINE)).abs() < 1e-12);
        assert_eq!(h.mouse((0.0, 0.0)), (false, FINE), "the view finer too");
        for _ in 0..1000 {
            h.wheel(1.0);
        }
        assert!((h.out - 0.45).abs() < 1e-12);
        for _ in 0..1000 {
            h.wheel(-1.0);
        }
        assert!(h.out.abs() < 1e-12);
    }

    #[test]
    fn with_q_the_mouse_turns_it_and_the_wheel_rolls_it() {
        let mut h = held();
        assert!(h.key(KeyCode::KeyQ, true));
        assert_eq!(h.mouse((100.0, 0.0)), (true, 1.0), "not the view");
        let yaw = h.turn.angle_between(DQuat::IDENTITY);
        assert!((yaw - 100.0 * PER_PIXEL).abs() < 1e-9, "{yaw}");
        h.turn = DQuat::IDENTITY;
        h.wheel(2.0);
        assert!((h.turn.angle_between(DQuat::IDENTITY) - 2.0 * ROLL).abs() < 1e-9);
        assert!(h.out.abs() < 1e-12, "rolled, not held out");
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
