//! The play scenes (`docs/g1-plan.md`, G1.0 and G1.10): what players will do, scripted. Each
//! has its world, its players and a script that decides every player's act from the tick
//! and what the server has (so a scene is the same on every run).
//!
//! G1 grows them as it adds what players do: *crowd* starts with the F5 avatars walking,
//! jumping and dropping crates among each other; *work* (G1.4g) has them strike, carry and
//! throw (`work.rs`).

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
    /// Whether it is played turned about the spawn (`Play::heading`) for its ensemble: a scene
    /// whose world is built away from the spawn plays the same at every heading.
    pub turned: bool,
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
pub const SCENES: [Scene; 10] = [
    Scene {
        name: "crowd",
        what: "8 players walking, jumping and dropping crates in one place",
        players: &["ada", "ben", "cai", "dee", "eli", "fay", "gus", "hal"],
        ticks: (1800, 300),
        build: world,
        script: crowd,
        turned: true,
    },
    Scene {
        name: "work",
        what: "8 players at work: 4 striking crates with axes and picks, 4 carrying and throwing boxes",
        players: &["ada", "ben", "cai", "dee", "eli", "fay", "gus", "hal"],
        ticks: (1800, 300),
        build: super::work::work_world,
        script: super::work::work,
        turned: true,
    },
    Scene {
        name: "lumberjack",
        what: "4 players in the forest with steel axes: each fells a notched tree and bucks its trunk",
        players: &["ada", "ben", "cai", "dee"],
        ticks: (1800, 300),
        build: super::lumberjack::lumber_world,
        script: super::lumberjack::lumberjack,
        turned: false,
    },
    Scene {
        name: "quarry",
        what: "4 players with picks digging the sides of a pit among boulders, its heaps growing",
        players: &["ada", "ben", "cai", "dee"],
        ticks: (1800, 300),
        build: super::quarry::quarry_world,
        script: super::quarry::quarry,
        turned: false,
    },
    Scene {
        name: "toolmaking",
        what: "4 players knapping a chert core, stripping bark, binding a flake on and digging with it",
        players: &["ada", "ben", "cai", "dee"],
        ticks: (1800, 1100),
        build: super::toolmaking::toolmaking_world,
        script: super::toolmaking::toolmaking,
        turned: false,
    },
    Scene {
        name: "tower",
        what: "a 20-storey steel frame of about 20,000 beams standing at rest (G2)",
        players: &["ada"],
        ticks: (600, 60),
        build: super::steel::tower_world,
        script: idle,
        turned: false,
    },
    Scene {
        name: "collapse",
        what: "the tower with a ground-floor column gone: its load finds other paths",
        players: &["ada"],
        ticks: (600, 120),
        build: super::steel::collapse_world,
        script: idle,
        turned: false,
    },
    Scene {
        name: "drop",
        what: "a 10 t block falling 3 m onto the middle of a 40 m steel bridge",
        players: &["ada"],
        ticks: (300, 120),
        build: super::steel::drop_world,
        script: idle,
        turned: false,
    },
    Scene {
        name: "builders",
        what: "8 players nailing crossed poles and taking them apart again, by hand",
        players: &["ada", "ben", "cai", "dee", "eli", "fay", "gus", "hal"],
        ticks: (1800, 600),
        build: super::builders::builders_world,
        script: super::builders::builders,
        turned: false,
    },
    Scene {
        name: "log_bridge",
        what: "2 players nailing a pole across two others, then crates on it until it breaks",
        players: &["ada", "ben"],
        ticks: (1800, 600),
        build: super::log_bridge::log_bridge_world,
        script: super::log_bridge::log_bridge,
        turned: false,
    },
];

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

/// A new world on the reference planet: a test server, creative (G1.4d).
pub fn world(terrain: Arc<Terrain>) -> ServerWorld {
    let mut world = ServerWorld::new(Data::base(), terrain, PLANET);
    world.creative = true;
    world
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

/// Players standing by (the structures' scenes: what they do is what the structures do).
fn idle(_: u32, play: &Play) -> Vec<Act> {
    vec![Act::default(); play.players()]
}
