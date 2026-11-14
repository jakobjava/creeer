//! Small mispredictions corrected in place, without a replay (G2.11 A5; Unreal's "runtime
//! correction" within its resimulation thresholds).
//!
//! Where a snapshot finds the avatar or a body near it off by more than its quanta but by little
//! (millimetres, centimetres a second), the client's prediction since that input is moved by the
//! difference, as though the body had always moved so (its solver's history moved with it,
//! `NodeState::moved_to`), and nothing is replayed. Measured before (*builders*, *crowd*, an
//! avatar walking into a crate): a lone avatar is never mispredicted, but in contact with
//! bodies the prediction and the server part by millimetres within a snapshot or two (contact
//! is stiff: ADR-0025's quantum-sized differences grow), and nine rewinds in ten were for
//! errors under 5 mm, each replaying every body near the avatar.
//!
//! Replayed, a difference is carried through the bodies' contacts over the inputs since; moved
//! in place, it is carried on as uniform motion. Over the few inputs pending on a good link the
//! two differ by far less than the error itself, which the next snapshot measures again. Over a
//! slow link's two dozen they do not: moved in place there, walking into crates corrected by up
//! to 17 cm at once (9 cm replayed), so beyond [`INPUTS`] pending it is replayed as before.

use super::Predicted;
use super::nearby::{Motion, agrees, motion};
use crate::world::DT;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::quant::BodyState;
use creeer_net::snapshot::Snapshot;
use creeer_physics::structure::history::NodeState;
use creeer_physics::world::Body;

/// The most a misprediction may be off, carried over the inputs pending, to be moved in place:
/// position (m), turn (rad), velocity (m/s) and spin (rad/s).
const POSITION: f64 = 0.005;
const TURN: f64 = 0.005;
const VELOCITY: f64 = 0.05;
const SPIN: f64 = 0.05;
/// The most inputs pending (since the snapshot's) a misprediction is moved over in place: a good
/// link's round trip (about 2 on a lossless link, 4 at 30 ms each way).
const INPUTS: u32 = 4;

/// How a body's motion is off the server's at an input.
#[derive(Clone, Copy, Debug)]
struct Delta {
    x: DVec3,
    q: DQuat,
    v: DVec3,
    w: DVec3,
}

impl Delta {
    /// The server's `state` less the predicted `own`.
    fn of((x, q, v, w): Motion, state: &BodyState) -> Self {
        Self {
            x: state.position() - x,
            q: (state.orientation() * q.conjugate()).normalize(),
            v: state.velocity() - v,
            w: state.spin() - w,
        }
    }

    /// Whether, carried over `tau` seconds, it is small enough to be moved in place.
    fn small(&self, tau: f64) -> bool {
        let turn = self.q.to_scaled_axis().length();
        self.x.length() + self.v.length() * tau <= POSITION
            && turn + self.w.length() * tau <= TURN
            && self.v.length() <= VELOCITY
            && self.w.length() <= SPIN
    }

    /// `m` moved by it, `tau` seconds after the input it was measured at (uniform motion).
    fn moved(&self, (x, q, v, w): Motion, tau: f64) -> Motion {
        let q = (DQuat::from_scaled_axis(self.w * tau) * self.q * q).normalize();
        (x + self.x + self.v * tau, q, v + self.v, w + self.w)
    }
}

/// `body` moved by `d` over `tau`, its history with it; a sleeping one is woken first.
fn move_body(body: &mut Body, d: &Delta, tau: f64) {
    body.wake();
    let to = d.moved(motion(body), tau);
    if let Some(own) = NodeState::of(&body.structure, &body.solver) {
        let h = body.solver.time_step();
        own.moved_to(to, h)
            .restore(&mut body.structure, &mut body.solver);
    }
}

impl Predicted {
    /// Moves the prediction since `snapshot`'s input in place where every misprediction it shows
    /// is small (the bodies near the avatar `avatar` being the same): true if it did.
    pub(super) fn nudge(&mut self, snapshot: &Snapshot, avatar: EntityId) -> bool {
        let last = snapshot.last_input;
        let (Some((mine, motions)), Some(newest)) =
            (self.history.after(last), self.history.newest())
        else {
            return false;
        };
        let Some(state) = snapshot.states.get(&avatar) else {
            return false;
        };
        if motions.len() != self.nearby.len()
            || self.world.bodies.len() != motions.len() + 1
            || newest.saturating_sub(last) > INPUTS
        {
            return false;
        }
        let tau = f64::from(newest.saturating_sub(last)) * DT;
        // Each one's difference (none where it agrees, or the snapshot left it out), and whether
        // the server has it asleep.
        let mut deltas: Vec<(Option<Delta>, bool)> = vec![(
            (!agrees(mine, state)).then(|| Delta::of(mine, state)),
            false,
        )];
        for ((id, m), want) in motions.iter().zip(&self.nearby) {
            if id != want {
                return false;
            }
            let stale = snapshot.stale.binary_search(id).is_ok();
            let s = snapshot.states.get(id).filter(|_| !stale);
            deltas.push(match s {
                Some(s) => ((!agrees(*m, s)).then(|| Delta::of(*m, s)), s.asleep),
                None => (None, false),
            });
        }
        if !(deltas.iter().filter_map(|d| d.0).all(|d| d.small(tau))) {
            return false;
        }
        // One the server has asleep and this client too stays as it is.
        for (d, body) in deltas.iter_mut().zip(&self.world.bodies) {
            if d.1 && body.sleep.asleep {
                d.0 = None;
            }
        }
        // The prediction now and the motions kept since, all moved; asleep where the server has it
        // asleep (as a body taken in, `nearby.rs`).
        for (k, (body, (d, asleep))) in self.world.bodies.iter_mut().zip(&deltas).enumerate() {
            if let Some(d) = d {
                move_body(body, d, tau);
            }
            if k > 0 && *asleep && !body.sleep.asleep {
                body.sleep_now();
            }
        }
        let index = |id: Option<EntityId>| match id {
            None => Some(0),
            Some(id) => self.nearby.iter().position(|n| *n == id).map(|i| i + 1),
        };
        let delta_of = |id: Option<EntityId>| index(id).and_then(|i| deltas[i].0);
        let since = |at: u32| f64::from(at.saturating_sub(last)) * DT;
        self.history.shift(last, |id, at, m| {
            delta_of(id).map_or(m, |d| d.moved(m, since(at)))
        });
        self.corrections.nudged += 1;
        true
    }
}
