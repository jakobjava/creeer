//! What the hands hold, placed in first person (G2.10, ADR-0036; the user's design,
//! 2026-10-30): held where the crosshair meets something within reach, resting against it (laid
//! on what lies under it, against a trunk, let down into a hole: `Connection::place_hold`);
//! meeting nothing, before the eyes. R stands it up or lays it down; Q with the mouse turns it
//! (across the view and up and down); the wheel slides it along itself (to lay a beam from one
//! post's top to the next); Ctrl makes all of it five times finer. J fastens it wherever it
//! touches a part, P sets a post standing in its hole; the hands keep it still meanwhile.

use creeer_net::entity::EntityId;
use creeer_render::Camera;
use creeer_server::Connection;
use std::f64::consts::FRAC_PI_2;
use winit::keyboard::KeyCode;

/// How far a notch of the wheel slides it along itself (m), at most either way.
const NOTCH: f64 = 0.05;
const MOST_SLID: f64 = 1.5;
/// How far the mouse turns it (rad per pixel).
const PER_PIXEL: f64 = 0.006;
/// Fine: everything this much slower.
const FINE: f64 = 0.2;

/// The player's handling of what their hands hold.
#[derive(Debug, Default)]
pub struct Holding {
    /// The thing held, as the server told it.
    load: Option<EntityId>,
    /// Its length turned from across the view (rad), tilted up from level (rad: R), and slid
    /// along itself (m: the wheel).
    yaw: f64,
    tilt: f64,
    slide: f64,
    /// The turn last asked of the hands (`Connection::place_hold`'s `from`).
    asked: Option<creeer_math::glam::DQuat>,
    /// Where the crosshair last rested it against something: J fastens it there (G2.11).
    touch: Option<creeer_math::glam::DVec3>,
    /// Nailed while held: the hands keep it still where it is (the crosshair free to aim at
    /// another crossing) until it is let go (G2.11).
    pinned: bool,
    /// Q held: the mouse turns it. Ctrl held: finer.
    turning: bool,
    pub fine: bool,
}

impl Holding {
    /// Whether anything is held.
    pub fn holding(&self) -> bool {
        self.load.is_some()
    }

    /// Where the crosshair rests what is held against something, if it does.
    pub fn touch(&self) -> Option<creeer_math::glam::DVec3> {
        self.touch.filter(|_| self.holding())
    }

    /// J pressed on what is held: from now on the hands keep it still where it is.
    pub fn pin(&mut self) {
        self.pinned = self.holding();
    }

    /// A key (R, Q, Ctrl); whether it was one for what is held. Q and Ctrl are followed with
    /// nothing held too (let go of while Q was down, the next thing taken turned with the mouse).
    pub fn key(&mut self, code: KeyCode, pressed: bool) -> bool {
        match code {
            KeyCode::KeyR if pressed && self.holding() => {
                self.tilt = if self.tilt > 0.25 * std::f64::consts::PI {
                    0.0
                } else {
                    FRAC_PI_2
                };
            }
            KeyCode::KeyR => return self.holding(),
            KeyCode::KeyQ => self.turning = pressed,
            KeyCode::ControlLeft | KeyCode::ControlRight => self.fine = pressed,
            _ => return false,
        }
        self.holding()
    }

    /// A turn of the wheel while something is held: slid along itself. Whether it took it.
    pub fn wheel(&mut self, notches: f64) -> bool {
        if !self.holding() {
            return false;
        }
        let gain = if self.fine { FINE } else { 1.0 };
        self.slide = (self.slide + notches * NOTCH * gain).clamp(-MOST_SLID, MOST_SLID);
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
        self.yaw += dx * gain;
        self.tilt = (self.tilt - dy * gain).clamp(-FRAC_PI_2, FRAC_PI_2);
        (true, fine)
    }

    /// Each frame: what is held now (taken: lying across the view, unslid), and the hold sent
    /// with the inputs from now on, resting on `surface` (what the crosshair meets:
    /// `Connection::surface_aimed`).
    pub fn update(
        &mut self,
        camera: &Camera,
        connection: &mut Connection,
        surface: Option<creeer_server::client::Surface>,
    ) {
        let view = camera.rotation();
        let load = connection.own_load();
        if load != self.load {
            // Taken, it is held the way it lay (G2.11: turned at once to lie across the view, a
            // pole swung round in the hands as it was taken), lying level or standing, as R
            // puts it (a pole taken from a pile, where it lay tilted 30° over another, was carried
            // so tilted).
            let (yaw, tilt) = connection.held_turn(view).unwrap_or_default();
            let tilt = if tilt.abs() > 0.25 * std::f64::consts::PI {
                FRAC_PI_2
            } else {
                0.0
            };
            (self.load, self.yaw, self.tilt, self.slide) = (load, yaw, tilt, 0.0);
            (self.asked, self.pinned) = (None, false);
        }
        self.touch = surface.filter(|_| self.load.is_some()).map(|s| s.point);
        let turned = (self.yaw, self.tilt, self.slide);
        let hold =
            (self.load).and_then(|_| connection.place_hold(view, surface, turned, self.asked));
        self.asked = hold.map(|h| h.turn);
        // Nailed while held: held still where it is (the server holds a steady hold where it is).
        let hold = hold.map(|h| creeer_net::message::Hold {
            steady: self.pinned,
            ..h
        });
        connection.set_hold(hold);
    }

    /// The keys' hint while something is held.
    pub fn hint(&self) -> Option<&'static str> {
        if self.pinned && self.holding() {
            return Some(
                "held still · J another nail where it touches · G let go: the nails hold it",
            );
        }
        self.holding().then_some(
            "aim where it goes · R stand/lie · Q+mouse turn · wheel slide · J nail where it touches · P set post · G put down",
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
        assert!(!h.key(KeyCode::KeyQ, true) && !h.key(KeyCode::KeyR, true));
        assert!(!h.wheel(1.0));
        assert_eq!(h.mouse((10.0, 0.0)), (false, 1.0));
        assert!(h.hint().is_none());
    }

    #[test]
    fn r_stands_it_up_and_lays_it_down() {
        let mut h = held();
        assert!(h.key(KeyCode::KeyR, true));
        assert!((h.tilt - FRAC_PI_2).abs() < 1e-12);
        assert!(h.key(KeyCode::KeyR, false), "a press toggles");
        h.key(KeyCode::KeyR, true);
        assert!(h.tilt.abs() < 1e-12);
    }

    #[test]
    fn the_wheel_slides_it_along_itself_finer_with_ctrl_and_within_bounds() {
        let mut h = held();
        assert!(h.wheel(1.0));
        assert!((h.slide - NOTCH).abs() < 1e-12);
        h.key(KeyCode::ControlLeft, true);
        h.wheel(1.0);
        assert!((h.slide - NOTCH * (1.0 + FINE)).abs() < 1e-12);
        assert_eq!(h.mouse((0.0, 0.0)), (false, FINE), "the view finer too");
        for _ in 0..1000 {
            h.wheel(-1.0);
        }
        assert!((h.slide + MOST_SLID).abs() < 1e-12);
    }

    #[test]
    fn with_q_the_mouse_turns_it_across_and_up() {
        let mut h = held();
        assert!(h.key(KeyCode::KeyQ, true));
        assert_eq!(h.mouse((100.0, -50.0)), (true, 1.0), "not the view");
        assert!((h.yaw - 100.0 * PER_PIXEL).abs() < 1e-12);
        assert!((h.tilt - 50.0 * PER_PIXEL).abs() < 1e-12);
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
