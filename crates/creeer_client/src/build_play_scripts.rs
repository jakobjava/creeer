//! The builds `build_play.rs` plays (G2.11 B), as a player builds them: a log bridge across a
//! gap, and a frame of posts with a rail and a brace.

use crate::build_play::Player;
use creeer_math::glam::DVec3;
use creeer_terrain::edits::Brush;
use winit::keyboard::KeyCode;

/// Q with the mouse this many pixels turns the held part a quarter turn (`holding.rs`).
const QUARTER: f64 = 262.0;

use creeer_net::entity::EntityId;

/// Drops the kit's `name` at `at` and returns what came (the body drawn nearest it that was not
/// there before).
fn drop_kit(p: &mut Player, name: &str, at: DVec3) -> Result<EntityId, String> {
    let known: Vec<_> = p.nearest(at).into_iter().map(|(id, _)| id).collect();
    p.kit(name, at)?;
    p.play(1.5, DVec3::ZERO);
    (p.nearest(at).into_iter())
        .find(|(id, _)| !known.contains(id))
        .map(|(id, _)| id)
        .ok_or(format!("the {name} did not come"))
}

/// Where `id` is drawn now.
fn now(p: &Player, id: EntityId) -> DVec3 {
    let c = &p.view.session.connection;
    c.poses().get(&id).map_or(DVec3::ZERO, |pose| pose.position)
}

/// Looks at `at` for `seconds`.
fn look(p: &mut Player, at: DVec3, seconds: f64) {
    p.look = at;
    p.play(seconds, DVec3::ZERO);
}

/// Takes `id` where it lies now (G, looking at it), and waits for the hands.
fn take(p: &mut Player, id: EntityId, name: &str) -> Result<(), String> {
    let at = now(p, id);
    look(p, at, 0.4);
    p.shot(&format!("{name}-looked-at"))?;
    p.key(KeyCode::KeyG);
    p.play(0.6, DVec3::ZERO);
    p.shot(&format!("{name}-taken"))
}

/// A log bridge: two 3 m poles from a pile carried 5 m and laid across a 1.8 m gap, and three
/// rails laid across them as a deck, each nailed at both ends.
///
/// # Errors
/// If a step cannot be played or a frame saved.
pub(crate) fn bridge(p: &mut Player) -> Result<(), String> {
    let start = p.me();
    let up = p.up();
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    // The gap: a trench about 1.5 m across at the top and 1.1 m deep, 5.5 m east, running north.
    let (a, b) = (at(p, 5.5, -3.0), at(p, 5.5, 3.0));
    p.dig(Brush::Capsule {
        a: a - up * 0.3,
        b: b - up * 0.3,
        radius: 0.8,
    });
    // Two poles lying side by side a metre and a half north (piled, the one beneath was pulled
    // out from under the other and came up steeply); three rails by the near bank.
    let first = drop_kit(p, "pole", at(p, 0.0, 1.6) + up * 0.1)?;
    let second = drop_kit(p, "pole", at(p, 0.0, 2.4) + up * 0.1)?;
    // (Dropped 0.4 m apart they fell across each other, and the one beneath stayed pinned.)
    let rails = [-1.4, -2.2, -3.0].map(|n| at(p, 2.8, n) + up * 0.1);
    let mut rail_ids = Vec::new();
    for r in rails {
        rail_ids.push(drop_kit(p, "rail", r)?);
    }
    look(p, at(p, 5.5, 0.0), 0.5);
    p.shot("the-site")?;
    // The first pole: taken, carried to the near bank, turned along the view, laid across.
    for (k, (pole, north)) in [(first, 0.0), (second, 0.8)].into_iter().enumerate() {
        let name = format!("pole{}", k + 1);
        let walk_back = (p.me() - now(p, pole)).reject_from(up).length() > 2.0;
        if walk_back {
            p.walk_to(now(p, pole), 1.4, 1.0);
        }
        take(p, pole, &name)?;
        look(p, p.eye() + p.east * 3.0, 0.8);
        p.shot(&format!("{name}-carried-level"))?;
        // (The trench's rounded rim begins about 4 m east: a stride short of it.)
        p.walk_to(at(p, 3.6, north), 0.15, 0.0);
        p.shot(&format!("{name}-at-the-bank"))?;
        p.turn_to(std::f64::consts::FRAC_PI_2);
        p.play(0.8, DVec3::ZERO);
        p.shot(&format!("{name}-turned-along"))?;
        // Aimed at the far bank's edge, then the gap's floor, then the near bank's edge.
        for (where_, e) in [("far-edge", 6.5), ("gap", 5.5), ("near-edge", 4.6)] {
            look(p, at(p, e, north), 1.5);
            p.shot(&format!("{name}-aimed-{where_}"))?;
        }
        look(p, at(p, 5.5, north), 1.5);
        p.shot(&format!("{name}-aimed-gap-again"))?;
        p.key(KeyCode::KeyG);
        p.play(0.5, DVec3::ZERO);
        p.shot(&format!("{name}-let-go"))?;
        p.play(3.0, DVec3::ZERO);
        p.shot(&format!("{name}-3s-on"))?;
    }
    // The deck: each rail taken, laid across both poles (lying across the view, looking east:
    // aimed into the gap beside the first pole, it is let down onto both), J: nailed where it
    // rests on the first; then J aimed where it crosses the second.
    for (k, rail) in rail_ids.into_iter().enumerate() {
        let name = format!("rail{}", k + 1);
        p.walk_to(now(p, rail), 1.3, 1.0);
        take(p, rail, &name)?;
        #[allow(clippy::cast_precision_loss)]
        let e = 4.6 + 0.3 * k as f64;
        p.walk_to(at(p, 3.8, 0.4), 0.15, 0.0);
        p.turn_to(0.0);
        look(p, at(p, e, 0.15), 1.5);
        p.shot(&format!("{name}-laid-across"))?;
        p.key(KeyCode::KeyJ);
        p.play(0.3, DVec3::ZERO);
        p.shot(&format!("{name}-j-held"))?;
        p.play(2.5, DVec3::ZERO);
        p.shot(&format!("{name}-after-nail1"))?;
        // The other crossing: the rail's top over the second pole, as the eye finds it.
        let crossing = at(p, e, 0.8);
        let top = (p.view.session.connection)
            .aim(crossing + up * 2.0, -up, 4.0)
            .map_or(crossing, |a| a.point);
        look(p, top, 0.5);
        p.shot(&format!("{name}-aimed-at-crossing2"))?;
        p.key(KeyCode::KeyJ);
        p.play(0.3, DVec3::ZERO);
        p.shot(&format!("{name}-j-at-crossing2"))?;
        p.play(2.5, DVec3::ZERO);
        p.shot(&format!("{name}-after-nail2"))?;
        if p.view.holding.holding() {
            p.key(KeyCode::KeyG);
            p.play(0.5, DVec3::ZERO);
        }
    }
    look(p, at(p, 5.5, 0.4), 1.0);
    p.shot("the-bridge")
}

/// A frame: a post stood up and let into a hole and set; a second; a rail held across both
/// posts' sides and nailed to each; a brace leant against a post and nailed.
///
/// # Errors
/// If a step cannot be played or a frame saved.
pub(crate) fn frame(p: &mut Player) -> Result<(), String> {
    let start = p.me();
    let up = p.up();
    let at = |p: &Player, e: f64, n: f64| p.ground_at(start, e, n);
    let holes = [at(p, 2.0, 0.7), at(p, 2.0, -0.7)];
    for mouth in holes {
        p.dig(Brush::Capsule {
            a: mouth,
            b: mouth - up * 0.6,
            radius: 0.15,
        });
    }
    let posts = [
        drop_kit(p, "post", at(p, 0.0, 1.4) + up * 0.15)?,
        drop_kit(p, "post", at(p, 0.0, -1.4) + up * 0.15)?,
    ];
    let rail = drop_kit(p, "rail", at(p, -0.8, 1.6) + up * 0.1)?;
    let brace = drop_kit(p, "rail", at(p, -0.8, -1.6) + up * 0.1)?;
    look(p, at(p, 2.0, 0.0), 0.5);
    p.shot("the-site")?;
    for (k, (post, mouth)) in posts.into_iter().zip(holes).enumerate() {
        let name = format!("post{}", k + 1);
        take(p, post, &name)?;
        look(p, mouth, 0.8);
        p.shot(&format!("{name}-aimed-at-hole-lying"))?;
        p.key(KeyCode::KeyR);
        p.play(0.4, DVec3::ZERO);
        p.shot(&format!("{name}-r-pressed"))?;
        p.play(1.5, DVec3::ZERO);
        p.shot(&format!("{name}-in-the-hole"))?;
        p.key(KeyCode::KeyP);
        p.play(1.0, DVec3::ZERO);
        p.shot(&format!("{name}-setting"))?;
        p.play(8.0, DVec3::ZERO);
        p.shot(&format!("{name}-set"))?;
    }
    // The rail across both posts' sides, a hand's breadth below their tops.
    take(p, rail, "rail")?;
    p.walk_to(at(p, 1.2, 0.0), 0.15, 0.3);
    let side = at(p, 1.94, 0.0) + up * 1.0;
    look(p, side, 1.5);
    p.shot("rail-aimed-between-posts")?;
    let faces = [0.7, -0.7].map(|n| at(p, 1.94, n) + up * 0.9);
    look(p, faces[0], 1.5);
    p.shot("rail-aimed-at-post1-side")?;
    for (k, face) in [(1, faces[0]), (2, faces[1])] {
        look(p, face, 0.5);
        p.shot(&format!("rail-aimed-to-nail{k}"))?;
        p.key(KeyCode::KeyJ);
        p.play(2.5, DVec3::ZERO);
        p.shot(&format!("rail-after-nail{k}"))?;
    }
    if p.view.holding.holding() {
        p.key(KeyCode::KeyG);
    }
    p.play(2.0, DVec3::ZERO);
    p.shot("rail-let-go")?;
    // The brace: leant from the ground beside the first post up against its side.
    p.walk_to(now(p, brace), 1.3, 1.0);
    take(p, brace, "brace")?;
    p.walk_to(at(p, 1.0, 1.5), 0.15, 0.3);
    look(p, at(p, 1.94, 0.7) + up * 0.6, 1.5);
    p.shot("brace-aimed-at-post1")?;
    p.turn(QUARTER * 0.5, -QUARTER * 0.5);
    p.play(1.0, DVec3::ZERO);
    p.shot("brace-turned-leaning")?;
    p.key(KeyCode::KeyJ);
    p.play(2.5, DVec3::ZERO);
    p.shot("brace-after-nail")?;
    if p.view.holding.holding() {
        p.key(KeyCode::KeyG);
    }
    p.play(2.0, DVec3::ZERO);
    p.shot("brace-let-go")?;
    look(p, at(p, 2.0, 0.0) + up * 0.6, 0.5);
    p.shot("the-frame")
}

impl Player {
    /// Q held, the mouse moved `(dx, dy)` pixels, Q let go.
    pub(crate) fn turn(&mut self, dx: f64, dy: f64) {
        let h = &mut self.view.holding;
        h.key(crate::bindings::Action::Turn, true);
        h.mouse((dx, dy));
        h.key(crate::bindings::Action::Turn, false);
    }

    /// Q with the mouse until what is held lies `yaw` from across the view (rad: a quarter turn
    /// lays it along the view), as a player turns it watching it.
    pub(crate) fn turn_to(&mut self, yaw: f64) {
        let view = self.camera().rotation();
        let now = (self.view.session.connection.held_turn(view)).map_or(0.0, |t| t.0);
        // (The mouse turns it 0.006 rad a pixel: `holding.rs`.)
        self.turn((yaw - now) / 0.006, 0.0);
    }
}
