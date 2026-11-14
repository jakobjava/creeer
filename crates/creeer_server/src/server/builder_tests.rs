//! A player building as the client builds (G2.10, G2.11 B), for the tests that play builds
//! through the client's own path: the crosshair's surface (`Connection::surface_aimed`), the
//! hold placed there (`Connection::place_hold`, as the client's `holding.rs` places it each
//! frame), and the commands the keys send.

use super::Game;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::link::Conditions;
use creeer_net::message::{Command, Target};
use creeer_sim::items::{Form, Item, Piece};
use creeer_sim::reach;
use creeer_terrain::edits::{Brush, Edit};

/// A view looking along `forward` (a camera's turn: −z ahead, y up), level with `up`.
pub(super) fn looking(forward: DVec3, up: DVec3) -> DQuat {
    let right = forward.cross(up).normalize();
    DQuat::from_mat3(&DMat3::from_cols(right, right.cross(forward), -forward)).normalize()
}

/// A green spruce pole `length` m long and `radius` m round.
pub(super) fn pole(length: f64, radius: f64) -> Item {
    Item::Piece(
        Piece::solid("spruce_green", 800.0, Form::Rod { radius, length }),
        Vec::new(),
    )
}

/// A squared spruce sill, 2.4 m long and 15 cm square.
pub(super) fn sill() -> Item {
    let size = DVec3::new(2.4, 0.15, 0.15);
    Item::Piece(
        Piece::solid("spruce_green", 800.0, Form::Block { size }),
        Vec::new(),
    )
}

/// Ada building in a creative world (nails at hand), and the way east and north.
pub(super) struct Builder {
    pub(super) game: Game,
    pub(super) ada: EntityId,
    pub(super) up: DVec3,
    pub(super) east: DVec3,
    pub(super) north: DVec3,
    /// The turn the last hold asked for (as the client's `holding.rs` keeps it).
    pub(super) asked: Option<DQuat>,
}

impl Builder {
    pub(super) fn new() -> Self {
        Self::over(None)
    }

    /// Ada over `link` (none: lossless).
    pub(super) fn over(link: Option<Conditions>) -> Self {
        let mut w = crate::world::tests::world();
        w.creative = true;
        let mut game = Game::new(w);
        game.join("ada", link);
        game.run(120, super::still);
        let ada = game.clients[0].0.welcome.as_ref().expect("joined").avatar;
        let up = game.avatar(0).normalize();
        let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
        Self {
            game,
            ada,
            up,
            east,
            north,
            asked: None,
        }
    }

    /// The ground under `at` m east and north of Ada.
    pub(super) fn ground(&self, east: f64, north: f64) -> DVec3 {
        let me = self.game.avatar(0);
        (self.game.server.world).ground_under(me + self.east * east + self.north * north)
    }

    /// A part laid on the ground there, along `axis`.
    pub(super) fn lay(&mut self, item: Item, at: DVec3, axis: DVec3) -> EntityId {
        let w = &mut self.game.server.world;
        let id = w.add_item(item, at + self.up * 0.1, true);
        let i = w.index(id).expect("there");
        let b = &mut w.world.bodies[i];
        let side = self.up.reject_from(axis).normalize();
        b.structure.orientation[0] =
            DQuat::from_mat3(&DMat3::from_cols(axis, side, axis.cross(side)));
        b.solver.reset_history(&b.structure);
        id
    }

    /// A hole dug at `mouth`, `depth` m deep.
    pub(super) fn dig(&mut self, mouth: DVec3, depth: f64) {
        let down = -mouth.normalize();
        (self.game.server.world.terrain).edit(Edit::Dig(Brush::Capsule {
            a: mouth,
            b: mouth + down * depth,
            radius: 0.15,
        }));
    }

    /// Ada's eyes, as her client draws her.
    pub(super) fn eye(&self) -> DVec3 {
        let me = self.game.clients[0].0.avatar().expect("drawn").position;
        reach::eye(me, self.up)
    }

    /// The view from Ada's eyes to `point`.
    pub(super) fn view_to(&self, point: DVec3) -> DQuat {
        looking((point - self.eye()).normalize(), self.up)
    }

    /// The turn (yaw, tilt 0, slide) that lays what is held, aimed at `from`, along to `to`:
    /// squared to them (Q, as a player turns it) and slid halfway (the wheel).
    pub(super) fn lying_along(&self, from: DVec3, to: DVec3) -> (f64, f64, f64) {
        let way = (to - from).normalize();
        let forward = (self.view_to(from) * DVec3::NEG_Z)
            .reject_from(self.up)
            .normalize();
        let across = forward.cross(self.up).normalize();
        let yaw = creeer_math::det::atan2(way.dot(forward), way.dot(across));
        (yaw, 0.0, 0.5 * (to - from).length())
    }

    /// Ada takes `id` in her hands, aiming at it as drawn.
    pub(super) fn take(&mut self, id: EntityId) {
        let client = &mut self.game.clients[0].0;
        let drawn = client.poses()[&id].position;
        let aimed = client
            .aim(drawn + self.up * 3.0, -self.up, 5.0)
            .expect("it");
        client.command(Command::Grab(Target {
            body: aimed.body,
            at: aimed.point,
            seen: aimed.seen,
        }));
        self.game.run(30, super::still);
        let told = self.told();
        assert_eq!(
            self.game.server.world.carried(self.ada),
            Some(id),
            "taken: {told:?}"
        );
        self.asked = None;
    }

    /// `ticks` with the crosshair on `point`, the held part placed there as the client places
    /// it: turned (yaw, tilt), slid along itself.
    pub(super) fn hold_at(&mut self, point: DVec3, turned: (f64, f64, f64), ticks: u32) {
        for _ in 0..ticks {
            let view = self.view_to(point);
            let client = &mut self.game.clients[0].0;
            let surface = client.surface_aimed(view, |_, _, _| None);
            let hold = client.place_hold(view, surface, turned, self.asked);
            self.asked = hold.map(|h| h.turn);
            client.set_hold(hold);
            self.game.tick(super::still);
        }
    }

    /// Ada walks towards `point` until she is within `within` m of it (level), then stands.
    pub(super) fn step_towards(&mut self, point: DVec3, within: f64) {
        for _ in 0..600 {
            let way = (point - self.game.avatar(0)).reject_from(self.up);
            if way.length() < within {
                break;
            }
            let walk = way.normalize();
            self.game.tick(|_| (walk, false));
        }
        self.game.run(60, super::still);
    }

    /// `ticks` holding what is held lying from `from` along to `to`, kept squared to them as the
    /// view moves (Q, as a player keeps it: its turn is the view's, and a turn fixed once
    /// drifted 8° as the body moved, a beam slid 0.72 m along it missing the second post).
    pub(super) fn hold_along(&mut self, from: DVec3, to: DVec3, ticks: u32) {
        for _ in 0..ticks {
            let turned = self.lying_along(from, to);
            self.hold_at(from, turned, 1);
        }
    }

    /// J at the held part where it lies across `top` (nailed through it, the thinner, into what
    /// is under it): the crosshair nudged towards Ada a centimetre at a time until it meets one
    /// of `parts`, as a player aims at what they see (aimed at the top itself, it passed over).
    pub(super) fn nail_at(&mut self, top: DVec3, parts: &[EntityId]) {
        let eye = self.eye();
        let toward = (eye - top).reject_from(self.up).normalize();
        let client = &mut self.game.clients[0].0;
        let aimed = (0..30)
            .filter_map(|k| {
                let p = top + toward * (0.01 * f64::from(k));
                client.aim(eye, (p - eye).normalize(), 4.0)
            })
            .find(|a| a.body.is_some_and(|id| parts.contains(&id)))
            .expect("the held part");
        // (No fastener named: the one the hand reaches for, as the client asks.)
        client.command(Command::Fasten {
            target: Target {
                body: aimed.body,
                at: aimed.point,
                seen: aimed.seen,
            },
            fastener: String::new(),
        });
    }

    /// A command about `body` at `at`, as the crosshair names it.
    pub(super) fn command(&mut self, body: EntityId, at: DVec3, make: impl Fn(Target) -> Command) {
        #[allow(clippy::cast_precision_loss)]
        let seen = self.game.server.world.tick as f64;
        let target = Target {
            body: Some(body),
            at,
            seen,
        };
        self.game.clients[0].0.command(make(target));
    }

    pub(super) fn centre(&self, id: EntityId) -> DVec3 {
        let w = &self.game.server.world;
        w.world.bodies[w.index(id).expect("there")]
            .structure
            .position[0]
    }

    /// How many of `parts` are still bodies of their own (fastened, all but one are merged).
    pub(super) fn bodies_left(&self, parts: &[EntityId]) -> usize {
        let w = &self.game.server.world;
        parts.iter().filter(|p| w.index(**p).is_some()).count()
    }

    /// What Ada was last told and refused.
    pub(super) fn told(&self) -> (Option<String>, Option<String>) {
        let c = &self.game.clients[0].0;
        (
            c.noted.as_ref().map(|n| n.0.clone()),
            c.refused.as_ref().map(|n| n.0.clone()),
        )
    }
}
