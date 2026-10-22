//! The play scenes (`docs/g1-plan.md`, G1.0 and G1.10): what players will do, scripted. Each
//! has its world, its players and a script that decides every player's act from the tick
//! and what the server has (so a scene is the same on every run).
//!
//! G1 grows them as it adds what players do: *crowd* starts with the F5 avatars walking,
//! jumping and dropping crates among each other.

use super::record::Act;
use super::{Play, Report, Tick, finish};
use crate::ServerWorld;
use crate::world::load_terrain;
use creeer_data::Data;
use creeer_math::det;
use creeer_math::glam::DVec3;
use creeer_net::message::Command;
use creeer_terrain::Terrain;
use creeer_terrain::geology::deposits::tangent_frame;
use std::sync::Arc;

/// A play scene.
pub struct Scene {
    /// Its name (`creeer_tools play --only <name>`).
    pub name: &'static str,
    /// What happens in it.
    pub what: &'static str,
    /// Its players' names.
    pub players: &'static [&'static str],
    /// Its ticks: full (gated in time) and short (CI: hashes and work).
    pub ticks: (u32, u32),
    /// Its world, on the server's own terrain.
    pub build: fn(Arc<Terrain>) -> ServerWorld,
    /// Every player's act in a tick.
    pub script: fn(u32, &Play) -> Vec<Act>,
}

impl std::fmt::Debug for Scene {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scene")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The planet every scene is played on.
pub const PLANET: &str = "planets/home.ron";

/// The scenes.
pub const SCENES: [Scene; 1] = [Scene {
    name: "crowd",
    what: "8 players walking, jumping and dropping crates in one place",
    players: &["ada", "ben", "cai", "dee", "eli", "fay", "gus", "hal"],
    ticks: (1800, 300),
    build: world,
    script: crowd,
}];

/// The scene called `name`.
pub fn scene(name: &str) -> Option<&'static Scene> {
    SCENES.iter().find(|s| s.name == name)
}

/// The reference planet's terrain, loaded anew (the server's, or the clients' own).
///
/// # Panics
/// If the base pack has no reference planet.
pub fn terrain() -> Arc<Terrain> {
    load_terrain(&Data::base(), PLANET).expect("the reference planet")
}

/// A new world on the reference planet.
pub fn world(terrain: Arc<Terrain>) -> ServerWorld {
    ServerWorld::new(Data::base(), terrain, PLANET)
}

/// Plays `scene` for `ticks` ticks.
pub fn run(scene: &Scene, ticks: u32) -> Report {
    run_with(scene, ticks, |_, _| {})
}

/// Plays `scene` for `ticks` ticks, telling `each` every tick's cost as it comes.
pub fn run_with(scene: &Scene, ticks: u32, each: impl FnMut(u32, Tick)) -> Report {
    run_turned(scene, ticks, 0.0, each)
}

/// Plays `scene` turned about the spawn by `heading` (rad: an ensemble's member, `Play::heading`)
/// for `ticks` ticks, telling `each` every tick's cost as it comes.
pub fn run_turned(
    scene: &Scene,
    ticks: u32,
    heading: f64,
    mut each: impl FnMut(u32, Tick),
) -> Report {
    let mut play = Play::new(
        (scene.build)(terrain()),
        scene.name,
        scene.players,
        &terrain(),
    );
    play.heading = heading;
    let start: Vec<_> = (0..play.players()).map(|k| play.bytes(k)).collect();
    let costs = (0..ticks)
        .map(|t| {
            let acts = (scene.script)(t, &play);
            let cost = play.tick(&acts);
            each(t, cost);
            cost
        })
        .collect();
    finish(play, costs, &start)
}

/// The world's east and north at the spawn point, turned by the play's heading.
fn compass(play: &Play) -> (DVec3, DVec3, DVec3) {
    let spawn = play.server.world.spawn();
    let (east, north) = tangent_frame(spawn.normalize());
    let (s, c) = det::sin_cos(play.heading);
    (spawn, east * c + north * s, north * c - east * s)
}

/// *crowd*: each player walks a circle of 4–11 m about the spawn (its own radius, speed and
/// way round), jumps every three seconds (each at its own moment), and every two seconds
/// one of them drops a crate ahead of itself.
fn crowd(tick: u32, play: &Play) -> Vec<Act> {
    let (spawn, east, north) = compass(play);
    let t = f64::from(tick) / 60.0;
    (0..play.players())
        .map(|k| {
            #[allow(clippy::cast_precision_loss)]
            let kf = k as f64;
            let radius = 4.0 + kf;
            let turn = if k % 2 == 0 { 1.0 } else { -1.0 };
            let angle = kf * 0.785 + turn * t * (1.2 + 0.1 * kf) / radius;
            let (s, c) = det::sin_cos(angle);
            let target = spawn + (east * c + north * s) * radius;
            let at = play.avatar(k);
            let up = at.normalize();
            let to = target - at;
            let to = to - up * to.dot(up);
            let walk = if to.length() > 0.05 {
                to.normalize() * (to.length() * 2.0).min(3.0)
            } else {
                DVec3::ZERO
            };
            let mut act = Act::walk(walk);
            let index = u32::try_from(k).unwrap_or(u32::MAX);
            act.jump = (tick + 23 * index).is_multiple_of(180);
            if tick % 120 == 60 && (tick / 120) as usize % play.players() == k {
                let ahead = walk.normalize_or_zero() * 3.0;
                let ground = play.server.world.ground_under(at + ahead);
                act.commands
                    .push(Command::Crate(ground + ground.normalize() * 1.0));
            }
            act
        })
        .collect()
}
