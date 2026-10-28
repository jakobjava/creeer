//! Steering what the hands hold (G2.10, ADR-0036): held where the player means it, as people
//! hold things before them. Taken, it is held where it was, relative to the view; it moves with
//! the view; the wheel pushes it away or draws it in; Q with the mouse turns it (left–right and
//! up–down about the view), Q with the wheel rolls it; Ctrl makes all of it five times finer;
//! H holds it still where it is (steady), the view and the other hand free to fasten it (J).
//! Sent with each input (`creeer_net::message::Hold`): the server's hands bring it there as
//! fast and as firmly as their strength allows (`creeer_server`'s `hands.rs`).

use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::message::Hold;
use creeer_render::Camera;
use creeer_server::Connection;
use winit::keyboard::KeyCode;

/// How far a notch of the wheel moves it (m), and how far from the eyes it is held (m).
const NOTCH: f64 = 0.02;
const NEAREST: f64 = 0.25;
const FARTHEST: f64 = 1.1;
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

    /// A key while something is held (Q, H, Ctrl); whether it was one.
    pub fn key(&mut self, code: KeyCode, pressed: bool) -> bool {
        if !self.holding() {
            return false;
        }
        match code {
            KeyCode::KeyQ => self.turning = pressed,
            KeyCode::ControlLeft | KeyCode::ControlRight => self.fine = pressed,
            KeyCode::KeyH if pressed => self.steady = !self.steady,
            KeyCode::KeyH => {}
            _ => return false,
        }
        true
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

    /// Each frame: what is held now (taken: held where it is, relative to the view `camera`),
    /// and the hold sent with the inputs from now on.
    pub fn update(&mut self, camera: &Camera, connection: &mut Connection) {
        let load = connection.own_load();
        if load != self.load {
            self.load = load;
            self.steady = false;
            self.take(camera, connection);
        }
        let (Some(_), Some(avatar)) = (self.load, connection.avatar()) else {
            connection.set_hold(None);
            return;
        };
        // Let go of steady: held from where it is now (it does not jump back).
        if !self.steady && self.was_steady {
            self.take(camera, connection);
        }
        self.was_steady = self.steady;
        let view = camera.rotation();
        connection.set_hold(Some(Hold {
            offset: camera.position + view * self.at - avatar.position,
            turn: (view * self.turn).normalize(),
            steady: self.steady,
        }));
    }

    /// Held from where it is now, relative to the view.
    fn take(&mut self, camera: &Camera, connection: &Connection) {
        let pose = self.load.and_then(|l| connection.poses().get(&l).copied());
        let back = camera.rotation().conjugate();
        (self.at, self.turn) = match pose {
            Some(p) => (back * (p.position - camera.position), back * p.orientation),
            None => (DVec3::new(0.0, -0.3, -0.6), DQuat::IDENTITY),
        };
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
