//! Building as the player builds, in first person (G2.10, the user's design, 2026-10-30): a
//! part taken, held where the crosshair meets something within reach and resting against it
//! (`Connection::surface_aimed`, `Connection::place_hold`), stood up (R) or lying, slid along
//! itself (the wheel), set (P) or fastened (J) where it touches: two posts set in their holes
//! and a beam nailed across their tops; a pole laid across two others and fastened.

use super::Game;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::message::{Command, Target};
use creeer_sim::items::{Form, Item, Piece};
use creeer_sim::reach;
use creeer_terrain::edits::{Brush, Edit};
use std::f64::consts::FRAC_PI_2;

/// A view looking along `forward` (a camera's turn: −z ahead, y up), level with `up`.
fn looking(forward: DVec3, up: DVec3) -> DQuat {
    let right = forward.cross(up).normalize();
    DQuat::from_mat3(&DMat3::from_cols(right, right.cross(forward), -forward)).normalize()
}

/// A green spruce pole `length` m long and `radius` m round.
fn pole(length: f64, radius: f64) -> Item {
    Item::Piece(
        Piece::solid("spruce_green", 800.0, Form::Rod { radius, length }),
        Vec::new(),
    )
}

/// A squared spruce sill, 2.4 m long and 15 cm square.
fn sill() -> Item {
    let size = DVec3::new(2.4, 0.15, 0.15);
    Item::Piece(
        Piece::solid("spruce_green", 800.0, Form::Block { size }),
        Vec::new(),
    )
}

/// Ada building in a creative world (nails at hand), and the way east and north.
struct Builder {
    game: Game,
    ada: EntityId,
    up: DVec3,
    east: DVec3,
    north: DVec3,
}

impl Builder {
    fn new() -> Self {
        let mut w = crate::world::tests::world();
        w.creative = true;
        let mut game = Game::new(w);
        game.join("ada", None);
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
        }
    }

    /// The ground under `at` m east and north of Ada.
    fn ground(&self, east: f64, north: f64) -> DVec3 {
        let me = self.game.avatar(0);
        (self.game.server.world).ground_under(me + self.east * east + self.north * north)
    }

    /// A part laid on the ground there, along `axis`.
    fn lay(&mut self, item: Item, at: DVec3, axis: DVec3) -> EntityId {
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
    fn dig(&mut self, mouth: DVec3, depth: f64) {
        let down = -mouth.normalize();
        (self.game.server.world.terrain).edit(Edit::Dig(Brush::Capsule {
            a: mouth,
            b: mouth + down * depth,
            radius: 0.15,
        }));
    }

    /// Ada's eyes, as her client draws her.
    fn eye(&self) -> DVec3 {
        let me = self.game.clients[0].0.avatar().expect("drawn").position;
        reach::eye(me, self.up)
    }

    /// The view from Ada's eyes to `point`.
    fn view_to(&self, point: DVec3) -> DQuat {
        looking((point - self.eye()).normalize(), self.up)
    }

    /// The turn (yaw, tilt 0, slide) that lays what is held, aimed at `from`, along to `to`:
    /// squared to them (Q, as a player turns it) and slid halfway (the wheel).
    fn lying_along(&self, from: DVec3, to: DVec3) -> (f64, f64, f64) {
        let way = (to - from).normalize();
        let forward = (self.view_to(from) * DVec3::NEG_Z)
            .reject_from(self.up)
            .normalize();
        let across = forward.cross(self.up).normalize();
        let yaw = creeer_math::det::atan2(way.dot(forward), way.dot(across));
        (yaw, 0.0, 0.5 * (to - from).length())
    }

    /// Ada takes `id` in her hands, aiming at it as drawn.
    fn take(&mut self, id: EntityId) {
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
        assert_eq!(self.game.server.world.carried(self.ada), Some(id), "taken");
    }

    /// `ticks` with the crosshair on `point`, the held part placed there as the client places
    /// it: turned (yaw, tilt), slid along itself.
    fn hold_at(&mut self, point: DVec3, turned: (f64, f64, f64), ticks: u32) {
        for _ in 0..ticks {
            let view = self.view_to(point);
            let client = &mut self.game.clients[0].0;
            let surface = client.surface_aimed(view, |_, _, _| None);
            let hold = client.place_hold(view, surface, turned);
            client.set_hold(hold);
            self.game.tick(super::still);
        }
    }

    /// Ada walks towards `point` until she is within `within` m of it (level), then stands.
    fn step_towards(&mut self, point: DVec3, within: f64) {
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
    fn hold_along(&mut self, from: DVec3, to: DVec3, ticks: u32) {
        for _ in 0..ticks {
            let turned = self.lying_along(from, to);
            self.hold_at(from, turned, 1);
        }
    }

    /// J at the held part where it lies across `top` (nailed through it, the thinner, into what
    /// is under it): the crosshair nudged towards Ada a centimetre at a time until it meets one
    /// of `parts`, as a player aims at what they see (aimed at the top itself, it passed over).
    fn nail_at(&mut self, top: DVec3, parts: &[EntityId]) {
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
        client.command(Command::Fasten {
            target: Target {
                body: aimed.body,
                at: aimed.point,
                seen: aimed.seen,
            },
            fastener: "nail_100".into(),
        });
    }

    /// A command about `body` at `at`, as the crosshair names it.
    fn command(&mut self, body: EntityId, at: DVec3, make: impl Fn(Target) -> Command) {
        #[allow(clippy::cast_precision_loss)]
        let seen = self.game.server.world.tick as f64;
        let target = Target {
            body: Some(body),
            at,
            seen,
        };
        self.game.clients[0].0.command(make(target));
    }

    fn centre(&self, id: EntityId) -> DVec3 {
        let w = &self.game.server.world;
        w.world.bodies[w.index(id).expect("there")]
            .structure
            .position[0]
    }

    /// How many of `parts` are still bodies of their own (fastened, all but one are merged).
    fn bodies_left(&self, parts: &[EntityId]) -> usize {
        let w = &self.game.server.world;
        parts.iter().filter(|p| w.index(**p).is_some()).count()
    }

    /// What Ada was last told and refused.
    fn told(&self) -> (Option<String>, Option<String>) {
        let c = &self.game.clients[0].0;
        (
            c.noted.as_ref().map(|n| n.0.clone()),
            c.refused.as_ref().map(|n| n.0.clone()),
        )
    }
}

#[test]
fn two_posts_set_in_their_holes_and_a_beam_nailed_across_their_tops() {
    let mut b = Builder::new();
    let holes = [b.ground(1.3, 0.7), b.ground(1.3, -0.7)];
    for mouth in holes {
        b.dig(mouth, 0.6);
    }
    let east = b.east;
    let posts = [
        b.lay(pole(1.6, 0.05), b.ground(0.0, -1.2), east),
        b.lay(pole(1.6, 0.05), b.ground(0.6, -1.4), east),
    ];
    // A 60 mm beam: a 100 mm nail through it reaches 40 mm into a post (24 mm needed).
    let beam = b.lay(pole(1.6, 0.03), b.ground(-0.6, -1.4), east);
    b.game.run(60, super::still);
    // Each post: taken, stood up (R), aimed at its hole's mouth (let down to the floor), set (P)
    // while held: the hands keep it still through the backfilling, then let go.
    let standing = (0.0, FRAC_PI_2, 0.0);
    for (post, mouth) in posts.into_iter().zip(holes) {
        b.take(post);
        b.hold_at(mouth, standing, 150);
        let w = &b.game.server.world;
        let q = w.world.bodies[w.index(post).expect("there")]
            .structure
            .orientation[0];
        let upright = (q * DVec3::X).dot(b.up).abs();
        println!(
            "post held in its hole: {:.1}° from upright",
            creeer_math::det::acos(upright).to_degrees()
        );
        assert!(upright > 0.95, "it stands: {upright}");
        let deep = (mouth - (b.centre(post) - b.up * 0.8)).dot(b.up);
        assert!(deep > 0.3, "its foot {deep:.2} m down");
        b.command(post, mouth, Command::SetPost);
        b.hold_at(mouth, standing, 9 * 60);
        assert!(
            b.game.server.world.carried(b.ada).is_none(),
            "set, let go: {:?}",
            b.told()
        );
    }
    // The beam: taken, aimed at the first post's top, squared to the frame as the view moves (Q)
    // and slid along
    // itself to rest on both; one J nails it to both.
    // Each post's top as seen: its rounded end's crown (aimed at its base, or at the crown
    // itself from a metre off, the crosshair met its near shoulder and the beam rested there,
    // 7 cm off the posts' middles, and slid off it while nailed).
    let crown = |p: EntityId| {
        let top = b.centre(p) + b.up * 0.847;
        let away = (top - b.game.avatar(0)).reject_from(b.up).normalize();
        top + away * 0.02
    };
    let tops = posts.map(crown);
    // Stepped up to the frame (holding the posts through their backfilling, she slid back about
    // 0.3 m each: out of reach of the beam's middle; docs/future-improvements.md).
    b.step_towards(0.5 * (tops[0] + tops[1]), 1.2);
    b.take(beam);
    b.hold_along(tops[0], tops[1], 180);
    b.nail_at(tops[0] + b.up * 0.03, &[beam, posts[1]]);
    b.hold_along(tops[0], tops[1], 3 * 60);
    // Nailed at both ends, let go (G): it must stand by itself.
    b.game.clients[0].0.command(Command::Drop);
    b.game.run(30, super::still);
    assert!(b.game.server.world.carried(b.ada).is_none(), "let go");
    let parts = [posts[0], posts[1], beam];
    assert_eq!(b.bodies_left(&parts), 1, "one frame: {:?}", b.told());
    // It stands: its highest point (the beam) where it was nailed, three seconds on.
    let frame = *(parts.iter())
        .find(|p| b.game.server.world.index(**p).is_some())
        .expect("the frame");
    let top = |b: &Builder| {
        let w = &b.game.server.world;
        let s = &w.world.bodies[w.index(frame).expect("there")].structure;
        (0..s.node_count())
            .map(|n| s.position[n].dot(b.up))
            .fold(f64::MIN, f64::max)
    };
    let before = top(&b);
    b.game.run(180, super::still);
    let sank = before - top(&b);
    assert!(sank < 0.02, "sank {sank} m");
}

/// A log bridge's deck begun: two squared sills laid along the way across (stringers; round logs
/// roll as the deck is nailed to the first, as the log bridge scene squares its sills) and a pole
/// laid over both, squared to them, set down on them and nailed to each by one J (G2's demo, the
/// user's choice, 2026-10-30).
#[test]
fn a_pole_laid_across_two_logs_and_nailed_to_both() {
    let mut b = Builder::new();
    let east = b.east;
    let stringers = [
        b.lay(sill(), b.ground(1.3, 0.6), east),
        b.lay(sill(), b.ground(1.3, -0.6), east),
    ];
    // A 60 mm pole: a 100 mm nail through it reaches 40 mm into the sill (24 mm needed).
    let deck = b.lay(pole(1.6, 0.03), b.ground(-0.4, -1.4), east);
    b.game.run(90, super::still);
    let tops = stringers.map(|s| b.centre(s) + b.up * 0.075);
    b.take(deck);
    b.hold_along(tops[0], tops[1], 180);
    b.nail_at(tops[0] + b.up * 0.04, &[deck, stringers[0]]);
    b.hold_along(tops[0], tops[1], 3 * 60);
    let parts = [stringers[0], stringers[1], deck];
    assert_eq!(b.bodies_left(&parts), 1, "one deck: {:?}", b.told());
}

/// The user's pole (G2.9: "how to dig it into the ground?"; 3 m of green spruce, 12 cm: 27 kg):
/// taken at its middle, stood up (R), aimed at its hole's mouth and let down to its floor, set
/// (P) while held, it stands. (Raised by its end and walked under, as before the first-person
/// placing, `raising_tests.rs`.)
#[test]
fn a_heavy_pole_stood_up_and_set_in_its_hole() {
    let mut b = Builder::new();
    let mouth = b.ground(1.4, 0.0);
    b.dig(mouth, 0.7);
    let east = b.east;
    let pole = b.lay(pole(3.0, 0.06), b.ground(0.0, -1.3), east);
    b.game.run(60, super::still);
    b.take(pole);
    let standing = (0.0, FRAC_PI_2, 0.0);
    b.hold_at(mouth, standing, 180);
    let w = &b.game.server.world;
    let q = w.world.bodies[w.index(pole).expect("there")]
        .structure
        .orientation[0];
    let upright = (q * DVec3::X).dot(b.up).abs();
    println!(
        "pole held in its hole: {:.1}° from upright",
        creeer_math::det::acos(upright).to_degrees()
    );
    assert!(upright > 0.95, "it stands: {upright}");
    let deep = (mouth - (b.centre(pole) - b.up * 1.5)).dot(b.up);
    assert!(deep > 0.6, "its foot {deep:.2} m down");
    b.command(pole, mouth, Command::SetPost);
    b.hold_at(mouth, standing, 9 * 60);
    assert!(
        b.game.server.world.carried(b.ada).is_none(),
        "set, let go: {:?}",
        b.told()
    );
    b.game.run(180, super::still);
    let w = &b.game.server.world;
    let s = &w.world.bodies[w.index(pole).expect("there")].structure;
    // (Set, it is a structure: its axis from its nodes, lowest to highest.)
    let along = |n: usize| s.position[n].dot(b.up);
    let (low, high) = (0..s.node_count()).fold((0, 0), |(l, h), n| {
        (
            if along(n) < along(l) { n } else { l },
            if along(n) > along(h) { n } else { h },
        )
    });
    let upright = (s.position[high] - s.position[low]).normalize().dot(b.up);
    assert!(upright > 0.95, "set, it stands: {upright}");
}
