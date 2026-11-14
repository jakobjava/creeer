//! Whole builds played as the player plays them (G2.11 B): parts taken from where they lie,
//! carried, placed where the crosshair meets something, turned and fastened, through the
//! client's own path (`builder_tests.rs`), on a lossless and a slow link.

use super::builder::{Builder, looking, pole};
use creeer_math::glam::{DQuat, DVec3};
use creeer_sim::reach;

impl Builder {
    /// `ticks` looking along `view` from the eyes, what is held placed as the client places it
    /// (turned `yaw` from across the view, tilted `tilt`), walking `walk` (m/s).
    fn look(&mut self, view: DQuat, (yaw, tilt): (f64, f64), walk: DVec3, ticks: u32) {
        for _ in 0..ticks {
            let client = &mut self.game.clients[0].0;
            let surface = client.surface_aimed(view, |_, _, _| None);
            let hold = client.place_hold(view, surface, (yaw, tilt, 0.0), self.asked);
            self.asked = hold.map(|h| h.turn);
            client.set_hold(hold);
            self.game.tick(|_| (walk, false));
        }
    }

    /// Carries what is held to within `within` m (level) of `to`, looking level the way she
    /// walks (3 m/s), as it lay in her hands (`held_turn`).
    fn carry_to(&mut self, to: DVec3, within: f64) {
        for _ in 0..600 {
            // Where she sees herself (her client's avatar: over a slow link the server's is
            // behind, and walking on to where it was, she went a metre past into the gap).
            let me = self.game.clients[0]
                .0
                .avatar()
                .map_or(self.game.avatar(0), |a| a.position);
            let way = (to - me).reject_from(self.up);
            if way.length() < within {
                break;
            }
            // Slowing the last metre and a half, as anyone walking up to an edge does (at 3 m/s
            // to the last, she went 1.8 m on into the gap: `docs/future-improvements.md`).
            let speed = (way.length() * 2.0).clamp(0.5, 3.0);
            // (Held the way it is, the player keeping it so.)
            let view = looking(way.normalize(), self.up);
            let turned = self.taken_turn(view);
            self.look(view, turned, way.normalize() * speed, 1);
        }
        let view = looking(self.east, self.up);
        let turned = self.taken_turn(view);
        self.look(view, turned, DVec3::ZERO, 60);
    }

    /// How the client holds what was just taken, seen along `view` (`holding.rs`): turned as it
    /// lay, lying level or standing.
    pub(super) fn taken_turn(&self, view: DQuat) -> (f64, f64) {
        let (yaw, tilt) = self.game.clients[0].0.held_turn(view).unwrap_or_default();
        let standing = tilt.abs() > 0.25 * std::f64::consts::PI;
        (
            yaw,
            if standing {
                std::f64::consts::FRAC_PI_2
            } else {
                0.0
            },
        )
    }
}

/// Where a pole lies: its two ends (its nodes' farthest along its long axis).
fn ends(b: &Builder, id: creeer_net::entity::EntityId) -> (DVec3, DVec3) {
    let w = &b.game.server.world;
    let s = &w.world.bodies[w.index(id).expect("there")].structure;
    let (p, q) = (s.position[0], s.orientation[0]);
    (p - q * DVec3::X * 1.5, p + q * DVec3::X * 1.5)
}

/// A log bridge's first two poles, as the player lays them: each taken from a pile, carried
/// 4 m to a 1.8 m gap, turned along the view (Q), aimed at the gap and let go (G). Returns
/// each pole's end heights over the banks' ground beside them (m), its angle from the way
/// across (°) and its speed 3 s after it was let go (m/s).
fn two_poles_across(link: Option<creeer_net::link::Conditions>) -> Vec<(f64, f64, f64, f64)> {
    let mut b = Builder::over(link);
    let (east, north, up) = (b.east, b.north, b.up);
    let start = b.game.avatar(0);
    let at = |b: &Builder, e: f64, n: f64| {
        (b.game.server.world).ground_under(start + east * e + north * n)
    };
    // The gap: a trench about 1.5 m across at the top and 1.1 m deep, 5.5 m east, running
    // north (dug wider, its rims rounded by the ground's 12.5 cm cells left a 3 m pole 30 cm on
    // each bank).
    let (a, c) = (at(&b, 5.5, -3.0), at(&b, 5.5, 3.0));
    (b.game.server.world.terrain).edit(creeer_terrain::edits::Edit::Dig(
        creeer_terrain::edits::Brush::Capsule {
            a: a - up * 0.3,
            b: c - up * 0.3,
            radius: 0.8,
        },
    ));
    let poles = [
        b.lay(
            pole(3.0, 0.06),
            at(&b, 0.0, 1.6),
            (east + north).normalize(),
        ),
        b.lay(pole(3.0, 0.06), at(&b, 0.5, 2.2), north),
    ];
    b.game.run(60, super::still);
    let mut out = Vec::new();
    let say = |b: &Builder, what: &str| {
        let me = b.game.avatar(0) - start;
        let over = (b.game.avatar(0) - b.game.server.world.ground_under(b.game.avatar(0))).dot(up);
        eprintln!(
            "{what}: she {:.2} E {:.2} N, {over:.2} m over the ground; told {:?}",
            me.dot(east),
            me.dot(north),
            b.told()
        );
    };
    for (pole, n) in poles.into_iter().zip([0.0, 0.8]) {
        b.step_towards(b.centre(pole), 1.2);
        say(&b, "by the pole");
        b.take(pole);
        // (The trench's rounded rim begins about 4 m east: a stride short of it.)
        b.carry_to(at(&b, 3.9, n), 0.1);
        say(&b, "at the bank");
        let view = b.view_to(at(&b, 5.5, n));
        b.look(view, (std::f64::consts::FRAC_PI_2, 0.0), DVec3::ZERO, 150);
        let (e0, e1) = ends(&b, pole);
        say(
            &b,
            &format!(
                "aimed: ends {:.2} {:.2} E",
                (e0 - start).dot(east),
                (e1 - start).dot(east)
            ),
        );
        b.game.clients[0]
            .0
            .command(creeer_net::message::Command::Drop);
        b.game.run(180, super::still);
        let (e0, e1) = ends(&b, pole);
        let over = |e: DVec3| (e - b.game.server.world.ground_under(e)).dot(up);
        let across = creeer_math::det::acos(((e1 - e0).normalize().dot(east)).abs().min(1.0));
        let w = &b.game.server.world;
        let speed = w.world.bodies[w.index(pole).expect("there")]
            .structure
            .velocity[0]
            .length();
        out.push((over(e0), over(e1), across.to_degrees(), speed));
    }
    out
}

/// Two poles laid across a gap as the player lays them, lossless and over 100 ms each way:
/// each rests on both banks, along the way across, still.
#[test]
fn two_poles_carried_and_laid_across_a_gap() {
    for link in [None, Some(super::held_view::slow())] {
        let laid = two_poles_across(link);
        eprintln!(
            "{}: {laid:?}",
            if link.is_some() { "slow" } else { "lossless" }
        );
        for (e0, e1, across, speed) in laid {
            assert!(
                e0.abs() < 0.15 && e1.abs() < 0.15,
                "on both banks: {e0} {e1}"
            );
            // (Along the way across as the eye lays it: within 20°.)
            assert!(across < 20.0 && speed < 0.01, "{across}° {speed} m/s");
        }
    }
}

/// The bridge's first step as the frames showed it (2026-11-13): a 3 m pole (27 kg) taken where
/// it lies across the way, looking level, then walked with 4 m. Before G2.11 it was turned at
/// once to lie across the view and lifted to turn in the air: it bobbed 0.4–1.8 m above the
/// eyes, 3–28° off level, for as long as it was held, out of sight in first person. Now: held as
/// it lay, lifted level before the chest, below the eyes, and carried there steadily.
#[test]
fn a_pole_taken_from_the_ground_is_carried_level_before_the_chest() {
    let mut b = Builder::new();
    let (east, north, up) = (b.east, b.north, b.up);
    let lies = b.ground(0.0, 1.6);
    let id = b.lay(pole(3.0, 0.06), lies, (east + north).normalize());
    b.game.run(60, super::still);
    b.take(id);
    let level = looking(east, up);
    let (yaw, tilt) = b.game.clients[0].0.held_turn(level).expect("held");
    assert!(
        (yaw.to_degrees() + 45.0).abs() < 3.0 && tilt.abs() < 0.05,
        "as it lay: {yaw} {tilt}"
    );
    let (mut tilted, mut above, mut ahead) = (0.0f64, f64::MIN, (f64::MAX, f64::MIN));
    for tick in 0..300u32 {
        let walk = if (60..140).contains(&tick) {
            east * 3.0
        } else {
            DVec3::ZERO
        };
        b.look(level, (yaw, tilt), walk, 1);
        let w = &b.game.server.world;
        let s = &w.world.bodies[w.index(id).expect("there")].structure;
        let (p, q) = (s.position[0], s.orientation[0]);
        tilted = tilted.max(creeer_math::det::asin(
            (q * DVec3::X).dot(up).abs().min(1.0),
        ));
        // (Lifted from the ground into the hands in its first second.)
        if tick >= 60 {
            let d = p - reach::eye(b.game.avatar(0), up);
            above = above.max(d.dot(up));
            ahead = (ahead.0.min(d.dot(east)), ahead.1.max(d.dot(east)));
        }
    }
    eprintln!(
        "tilted {:.1}°, at most {above:.2} m from the eyes' height, {ahead:?} ahead",
        tilted.to_degrees()
    );
    assert!(
        w_held(&b) && tilted.to_degrees() < 2.0,
        "level: {}°",
        tilted.to_degrees()
    );
    assert!(
        above < -0.3 && ahead.1 - ahead.0 < 0.1,
        "before the chest, steady: {above} {ahead:?}"
    );
    let walked = (b.game.avatar(0) - lies).dot(east);
    assert!(walked > 3.7, "walked {walked} m with it");
}

fn w_held(b: &Builder) -> bool {
    b.game.server.world.carried(b.ada).is_some()
}
