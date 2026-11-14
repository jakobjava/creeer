//! Moving and looking as the player meets them (G2.11, stream A4; ADR-0042): a player on a
//! course of things to walk over (logs, a slab, slopes), played over the real server and
//! client (`Play`), on a lossless or a slow link, drawn frame by frame as a screen draws
//! faster than the server ticks. What the player's eye and hands get is measured from it: the
//! avatar as the client draws it each frame (the first-person camera sits at its eyes), and the
//! body as the server has it each tick.
//!
//! The courses are built on the play scenes' world, east of the spawn, where the ground is
//! level to within 1.5°.

use super::Play;
use super::record::Act;
use super::scenes::{terrain, world};
use crate::ServerWorld;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::interp::Pose;
use creeer_net::link::Conditions;
use creeer_net::message::Command;
use creeer_sim::items::{Form, Item, Piece};
use creeer_terrain::edits::{Brush, Edit, Edits};
use creeer_terrain::geology::deposits::tangent_frame;
use std::time::Duration;

/// A screen drawing faster than the server ticks (frames a second).
pub const FPS: f64 = 144.0;

/// 100–130 ms each way, one packet in twenty lost: the slow link the prediction is gated on
/// (ADR-0025).
pub fn slow() -> Conditions {
    Conditions {
        latency: Duration::from_millis(100),
        jitter: Duration::from_millis(30),
        loss: 0.05,
    }
}

/// What lies on a course, `east` metres east of the spawn.
#[derive(Clone, Copy, Debug)]
pub enum Thing {
    /// A green spruce log `radius` m round, 2 m long, lying across the way (north–south).
    Log { east: f64, radius: f64 },
    /// A squared green spruce block, `size` m (along the way, high, across), lying flat: a step
    /// onto something wide (a deck's height).
    Slab { east: f64, size: DVec3 },
    /// The ground raised as a dome: a ball `radius` m round whose top stands `high` m above the
    /// ground, its slope from the level at the top to `asin(√(2rh − h²)/r)` at its foot.
    Mound { east: f64, radius: f64, high: f64 },
    /// A board `width` m wide and 50 mm thick along the way, from `east` over `span` m between
    /// two squared sills (150 mm) lying across it, nailed to both: a plank bridge, as *log
    /// bridge*'s stringer (150 mm wide), or a deck (wider). The board's entity is the bridge.
    Bridge { east: f64, span: f64, width: f64 },
}

/// A squared green spruce block of `size` m (its x along its length).
fn block(size: DVec3) -> Item {
    Item::Piece(
        Piece::solid("spruce_green", 800.0, Form::Block { size }),
        Vec::new(),
    )
}

/// A board over two sills across the way, nailed to both (`Thing::Bridge`).
fn bridge(
    w: &mut ServerWorld,
    (east, north): (DVec3, DVec3),
    (from, span, width): (f64, f64, f64),
) -> EntityId {
    let spawn = w.spawn();
    let up = spawn.normalize();
    let half = 0.075;
    let middle = w.ground_under(spawn + east * (from + 0.5 * span));
    let mut top = f64::MIN;
    let mut sills = Vec::new();
    for side in [-0.5, 0.5] {
        let at = w.ground_under(middle + east * (side * span)) + up * (half + 0.001);
        top = top.max((at - middle).dot(up) + half);
        sills.push(lay(
            w,
            block(DVec3::new(1.2, 2.0 * half, 2.0 * half)),
            at,
            north,
        ));
    }
    let size = DVec3::new(span + 0.6, 0.05, width);
    let board = lay(w, block(size), middle + up * (top + 0.026), east);
    // Settled on them (the ground slopes a little: the lower sill a few millimetres below).
    for _ in 0..60 {
        w.step();
    }
    for sill in sills {
        w.nail_for_course(board, sill).expect("nailed");
    }
    board
}

/// A course: the game, the way east and north at the spawn, and the things on it by entity.
#[derive(Debug)]
pub struct Course {
    /// The game.
    pub play: Play,
    /// Up, east and north at the spawn.
    pub up: DVec3,
    pub east: DVec3,
    pub north: DVec3,
    /// The bodies put on it, in the order asked (a mound has none).
    pub things: Vec<Option<EntityId>>,
}

/// A part lying at `at`, its length (x) along `along`, its y up.
fn lay(w: &mut ServerWorld, item: Item, at: DVec3, along: DVec3) -> EntityId {
    let up = at.normalize();
    let id = w.add_item(item, at, false);
    if let Some(i) = w.index(id) {
        let x = along.reject_from(up).normalize();
        let b = &mut w.world.bodies[i];
        b.structure.orientation[0] = DQuat::from_mat3(&DMat3::from_cols(x, up, x.cross(up)));
        b.solver.reset_history(&b.structure);
    }
    id
}

/// A course of `things`, its players `players` joined over `link`.
///
/// # Panics
/// If the players do not join.
pub fn course(things: &[Thing], players: &[&str], link: Conditions) -> Course {
    let terrain = terrain();
    let mut w = world(terrain.clone());
    let spawn = w.spawn();
    let up = spawn.normalize();
    let (east, north) = tangent_frame(up);
    let mut ids = Vec::new();
    for thing in things {
        let id = match *thing {
            Thing::Log { east: e, radius } => {
                let ground = w.ground_under(spawn + east * e);
                let rod = Form::Rod {
                    radius,
                    length: 2.0,
                };
                let log = Item::Piece(Piece::solid("spruce_green", 800.0, rod), Vec::new());
                Some(lay(&mut w, log, ground + up * (radius + 0.002), north))
            }
            Thing::Slab { east: e, size } => {
                let ground = w.ground_under(spawn + east * (e + 0.5 * size.x));
                Some(lay(
                    &mut w,
                    block(size),
                    ground + up * (0.5 * size.y + 0.002),
                    east,
                ))
            }
            Thing::Bridge {
                east: e,
                span,
                width,
            } => Some(bridge(&mut w, (east, north), (e, span, width))),
            Thing::Mound {
                east: e,
                radius,
                high,
            } => {
                let top = w.ground_under(spawn + east * e) + up * high;
                let mut edits = Edits::default();
                edits.push(Edit::Fill(
                    Brush::Sphere {
                        center: top - up * radius,
                        radius,
                    },
                    w.terrain.rock_at(top - up * (high + 1.0)).unwrap_or(0),
                ));
                w.apply(&Command::Edit(edits.to_bytes()), None)
                    .expect("the mound");
                None
            }
        };
        ids.push(id);
    }
    let play = Play::over(w, ("walking", players), &terrain, link);
    Course {
        play,
        up,
        east,
        north,
        things: ids,
    }
}

/// A frame as the client drew it: the last tick played before it, the time (s), the player's
/// avatar as drawn (its centre; the first-person camera is at its eyes), and whether the
/// player was down.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub tick: u32,
    pub time: f64,
    pub drawn: Pose,
    pub fallen: bool,
}

/// A tick as the server has player 0: its body's centre and velocity, whether it is down.
#[derive(Clone, Copy, Debug)]
pub struct Ticked {
    pub tick: u32,
    pub at: DVec3,
    pub velocity: DVec3,
    pub fallen: bool,
    /// How far its body leans from upright (rad).
    pub lean: f64,
}

/// Plays `ticks` of `script` (each player's act in a tick) on `course`, player 0 drawn [`FPS`]
/// times a second: its frames, and its ticks as the server had them.
pub fn walk(
    course: &mut Course,
    ticks: u32,
    script: impl Fn(u32, &Course) -> Vec<Act>,
) -> (Vec<Frame>, Vec<Ticked>) {
    let (dt, mut t, mut time, mut drawn_to) = (1.0 / FPS, 0u32, 0.0, 0.0);
    let (mut frames, mut ticked) = (Vec::new(), Vec::new());
    while t < ticks {
        time += dt;
        while f64::from(t) / 60.0 < time && t < ticks {
            let acts = script(t, course);
            course.play.tick(&acts);
            ticked.push(server_side(course, t));
            drawn_to = f64::from(t) / 60.0;
            t += 1;
        }
        // The client drawing on between ticks, as each frame.
        course.play.draw_later(0, time - drawn_to);
        drawn_to = time;
        let client = course.play.client(0);
        if let Some(drawn) = client.avatar() {
            frames.push(Frame {
                tick: t - 1,
                time,
                drawn,
                fallen: client.fallen,
            });
        }
    }
    (frames, ticked)
}

/// Player 0 as the server has it after tick `tick`.
fn server_side(course: &Course, tick: u32) -> Ticked {
    let w = &course.play.server.world;
    let id = course
        .play
        .client(0)
        .welcome
        .as_ref()
        .expect("joined")
        .avatar;
    let i = w.index(id).expect("in the world");
    let s = &w.world.bodies[i].structure;
    let up = creeer_sim::character::plumb(&w.world, s.position[0]);
    Ticked {
        tick,
        at: s.position[0],
        velocity: s.velocity[0],
        fallen: w
            .person(id)
            .is_some_and(crate::world::people::Person::fallen),
        lean: creeer_math::det::acos((s.orientation[0] * DVec3::Y).dot(up).clamp(-1.0, 1.0)),
    }
}
