//! Frames of the things a player handles and stands among (G2.11 A2, ADR-0041), for comparing
//! how they look before and after a change: `creeer_client --planet <view> <file.png>`, and
//! what a view costs a frame (`--frames n`, [`time_frames`]).
//!
//! - `parts`: every part of the creative kit dropped in a row on the ground before the player
//!   and seen from their eyes, 1.5–3 m off (the rods, the sill, a stick, stones, a crate);
//! - `nailed`: two green spruce poles laid across each other (the *builders* scene's), nailed
//!   where they cross by the player, seen from their eyes;
//! - `forest`: from eye height 40 m into the forest beyond the spawn's clearing;
//! - `tree`: the nearest tree there at least 25 cm thick, from 8 m off at eye height;
//! - `felled`: *lumberjack*'s first tree felled and limbed by `--play-bench fell`'s feller;
//! - `bridge`: *log bridge*'s plank nailed to its sills by the player and loaded with crates
//!   until it breaks (`BRIDGE_CRATES` at most, or `CREEER_BRIDGE_CRATES`: 0 for the bridge
//!   as built), seen from beside it.

use crate::planet_view::PlanetView;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::message::{Command, Target};
use creeer_render::Camera;
use std::time::Duration;

/// The views this module takes.
pub(crate) const VIEWS: [&str; 6] = ["parts", "nailed", "forest", "tree", "felled", "bridge"];

/// The world a view is played in: the builders' poles for `nailed`, else the creative
/// singleplayer game.
pub(crate) fn view(name: &str) -> Result<PlanetView, String> {
    let scene = match name {
        "nailed" => "builders",
        "felled" => "lumberjack",
        "bridge" => "log_bridge",
        _ => return PlanetView::singleplayer(true),
    };
    let data = crate::content::data();
    let terrain = creeer_server::world::load_terrain(&data, crate::planet_net::PLANET)?;
    let scene = creeer_server::play::scenes::scene(scene).ok_or("no such scene")?;
    let mut world = (scene.build)(terrain);
    // The feller takes the creative steel axe, as `--play-bench fell`'s does.
    world.creative = true;
    PlanetView::hosting(world)
}

/// A tree felled and limbed by `--play-bench fell`'s feller (40 s of play), seen over its
/// shoulder from where it stands: the trunk, its cut ends, the branches beside it, the stump.
fn felled(view: &mut PlanetView) -> Result<Camera, String> {
    let mut player = crate::play_scripts::Player::default();
    let step = Duration::from_millis(16);
    let start = std::time::Instant::now();
    let mut look = DVec3::ZERO;
    while start.elapsed().as_secs_f64() < 40.0 {
        let me = view
            .session
            .connection
            .avatar()
            .ok_or("no avatar")?
            .position;
        player.t = start.elapsed().as_secs_f64();
        let (walk, at) = crate::play_scripts::fell(view, &mut player, me);
        look = at;
        view.session
            .update(step.as_secs_f64(), (walk, false, DVec3::ZERO));
        view.update_trees(me);
        std::thread::sleep(step);
    }
    let me = view
        .session
        .connection
        .avatar()
        .ok_or("no avatar")?
        .position;
    let up = me.normalize();
    let eye = creeer_sim::reach::eye(me, up);
    let to = (look - eye).normalize_or(up.any_orthonormal_vector());
    Ok(crate::planet_view::place(eye - to * 2.5 + up * 0.6, to))
}

/// The camera of view `name` (one of [`VIEWS`]), its scene built and played to it.
///
/// # Errors
/// If the player has not come, or the scene cannot be made.
pub(crate) fn camera(view: &mut PlanetView, name: &str) -> Result<Camera, String> {
    use crate::planet_shot_build::play;
    play(view, 2.0)?;
    let me = (view.session.connection.avatar())
        .ok_or("no avatar")?
        .position;
    let up = me.normalize();
    let terrain = std::sync::Arc::clone(&view.terrain);
    let ground =
        |p: DVec3| p.normalize() * (terrain.planet().datum.radius(p) + terrain.surface_height(p));
    match name {
        "parts" => parts(view, me, ground),
        "nailed" => nailed(view),
        "felled" => felled(view),
        "bridge" => bridge(view),
        "tree" => {
            // The nearest tree at least 25 cm thick 40 m east, from 8 m off at eye height.
            let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
            let at = ground(me + east * 40.0);
            view.update_trees(at);
            let trees = view.trees.as_ref().ok_or("no trees")?;
            let tree = trees.nearest(at, 0.25).ok_or("no tree there")?;
            let side = (east * 0.6 - north * 0.8).normalize();
            let eye = ground(tree.base - side * 8.0);
            let eye = eye + eye.normalize() * 1.7;
            let target = tree.base + tree.base.normalize() * 6.0;
            Ok(crate::planet_view::place(eye, (target - eye).normalize()))
        }
        _ => {
            let (east, _) = creeer_terrain::geology::deposits::tangent_frame(up);
            let at = ground(me + east * 40.0);
            let eye = at + at.normalize() * 1.7;
            Ok(crate::planet_view::place(eye, east))
        }
    }
}

/// The kit's parts in a row across the player's view, lying as they fell.
fn parts(
    view: &mut PlanetView,
    me: DVec3,
    ground: impl Fn(DVec3) -> DVec3,
) -> Result<Camera, String> {
    let up = me.normalize();
    // A rod lies along its frame's x, the frame stood upright where it is dropped.
    let along = (DQuat::from_rotation_arc(DVec3::Y, up) * DVec3::X)
        .reject_from(up)
        .normalize();
    let across = up.cross(along).normalize();
    let kit = creeer_sim::items::kit::kit(&creeer_data::Data::base())?;
    let order = [
        ("post", 1.2),
        ("rail", 1.55),
        ("pole", 1.95),
        ("sill", 2.45),
        ("stick", 2.85),
        ("cobble", 3.1),
        ("pebble", 3.25),
        ("crate", 3.7),
    ];
    for (name, out) in order {
        let k = (kit.iter().position(|k| k.name == name)).ok_or(format!("no {name} in the kit"))?;
        let along_by = if matches!(name, "cobble" | "pebble") {
            0.5
        } else {
            0.0
        };
        let at = ground(me + across * out + along * along_by) + up * 0.25;
        view.send(Command::Kit {
            item: u8::try_from(k).map_err(|e| e.to_string())?,
            at,
        });
    }
    crate::planet_shot_build::play(view, 3.0)?;
    let eye = creeer_sim::reach::eye(me, up);
    let target = ground(me + across * 2.3);
    Ok(crate::planet_view::place(eye, (target - eye).normalize()))
}

/// The first pair of the builders' poles nailed where they cross, from the player's eyes.
fn nailed(view: &mut PlanetView) -> Result<Camera, String> {
    // The scene's first pair (ids 1 and 2: made before anyone joined), east of the spawn.
    let (lower, upper) = (EntityId(1), EntityId(2));
    let poses = view.session.connection.poses();
    let a = poses.get(&lower).ok_or("no lower pole")?.position;
    let b = poses.get(&upper).ok_or("no upper pole")?.position;
    let crossing = 0.5 * (a + b);
    // Walk up to within a metre of where they cross.
    let step = Duration::from_millis(10);
    for _ in 0..600 {
        let at = view
            .session
            .connection
            .avatar()
            .ok_or("no avatar")?
            .position;
        let up = at.normalize();
        let off = (crossing - at).reject_from(up);
        if off.length() < 1.1 {
            break;
        }
        view.session.update(
            step.as_secs_f64(),
            (off.normalize() * 1.2, false, DVec3::ZERO),
        );
        std::thread::sleep(step);
    }
    crate::planet_shot_build::play(view, 1.0)?;
    let c = &view.session.connection;
    let up = crossing.normalize();
    let aimed = (c.aim(crossing + up * 2.0, -up, 3.0)).ok_or("the poles are not there")?;
    view.send(Command::Fasten {
        target: Target {
            body: aimed.body,
            at: aimed.point,
            seen: aimed.seen,
        },
        fastener: "nail_100".into(),
    });
    crate::planet_shot_build::play(view, 4.0)?;
    if let Some((why, _)) = &view.session.connection.refused {
        return Err(format!("the nail was refused: {why}"));
    }
    let me = (view.session.connection.avatar())
        .ok_or("no avatar")?
        .position;
    let eye = creeer_sim::reach::eye(me, up);
    Ok(crate::planet_view::place(eye, (crossing - eye).normalize()))
}

/// The most crates the `bridge` view drops on its plank.
const BRIDGE_CRATES: usize = 8;

/// *log bridge*'s plank (entity 3) nailed at both ends to its sills (1, 2), then loaded with
/// crates on its middle, one every two seconds, until it has come apart (a new entity of a
/// piece) or [`BRIDGE_CRATES`] lie on it; seen from beside it.
fn bridge(view: &mut PlanetView) -> Result<Camera, String> {
    use crate::planet_shot_build::play;
    let plank = EntityId(3);
    let middle = |view: &PlanetView| {
        let c = &view.session.connection;
        c.poses().get(&plank).map(|p| p.position)
    };
    let at = middle(view).ok_or("no plank")?;
    let up = at.normalize();
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    // Beside the plank's middle, within reach of both its ends.
    walk_to(view, at - north * 1.0)?;
    for sill in [EntityId(1), EntityId(2)] {
        let c = &view.session.connection;
        let on = c.poses().get(&sill).ok_or("no sill")?.position;
        let aimed = (c.aim(on + up * 1.5, -up, 2.0)).ok_or("the plank is not there")?;
        view.send(Command::Fasten {
            target: Target {
                body: aimed.body,
                at: aimed.point,
                seen: aimed.seen,
            },
            fastener: "nail_100".into(),
        });
        play(view, 3.0)?;
    }
    let crates = std::env::var("CREEER_BRIDGE_CRATES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(BRIDGE_CRATES);
    let mut dropped = 0;
    while dropped < crates {
        let Some(on) = middle(view) else { break };
        view.send(Command::Crate(on + up * 1.5));
        dropped += 1;
        play(view, 2.0)?;
        if view
            .session
            .connection
            .broken
            .values()
            .any(|b| !b.is_empty())
        {
            break;
        }
    }
    play(view, 2.0)?;
    let broken = view
        .session
        .connection
        .broken
        .values()
        .any(|b| !b.is_empty());
    eprintln!("bridge: {dropped} crates dropped, broken: {broken}");
    let eye = at - north * 5.5 + east * 2.0 + up * 3.0;
    Ok(crate::planet_view::place(eye, (at - eye).normalize()))
}

/// Walks the player to within 0.3 m of `to` (along the ground), 10 s at most.
fn walk_to(view: &mut PlanetView, to: DVec3) -> Result<(), String> {
    let step = Duration::from_millis(10);
    for _ in 0..1000 {
        let at = view
            .session
            .connection
            .avatar()
            .ok_or("no avatar")?
            .position;
        let off = (to - at).reject_from(at.normalize());
        if off.length() < 0.3 {
            break;
        }
        view.session.update(
            step.as_secs_f64(),
            (off.normalize() * 1.2, false, DVec3::ZERO),
        );
        std::thread::sleep(step);
    }
    crate::planet_shot_build::play(view, 0.5)
}

/// What a view's frames are drawn with (`planet_shot.rs`).
pub(crate) struct Timed<'a> {
    pub camera: &'a Camera,
    pub lighting: &'a creeer_render::Lighting,
    pub sky: &'a creeer_render::Sky,
    pub meshes: &'a crate::DemoMeshes,
    pub drawn: &'a [crate::people::DrawnPerson],
    pub ground: &'a crate::people_ground::PlanetGround<'a>,
}

/// Draws `view` `n` times more, each from the snapshot on, and prints what a frame cost: its
/// wall time (p50, p99), the CPU's share (the snapshot and the scene made of it, best of the
/// frames), the instances on screen and in shadows, and each pass's GPU time.
pub(crate) fn time_frames(
    n: usize,
    view: &PlanetView,
    (renderer, people): (&mut creeer_render::Renderer, &mut crate::people::People),
    at: &Timed<'_>,
) {
    use std::time::Instant;
    let mut stats = creeer_core::TimingStats::new(n);
    let (mut told, mut built) = (f64::MAX, f64::MAX);
    let mut counts = (0, 0);
    for _ in 0..n {
        let started = Instant::now();
        let snapshot = view.snapshot();
        told = told.min(started.elapsed().as_secs_f64() * 1e3);
        counts = (snapshot.things.len(), snapshot.foliage.len());
        let lap = Instant::now();
        let mut scene = creeer_render::Scene::default();
        let pair = (&snapshot, &snapshot, 1.0);
        let off = crate::demo::Overlay::Off;
        crate::physics_view::draw(&mut scene, at.meshes, DVec3::ZERO, pair, off);
        built = built.min(lap.elapsed().as_secs_f64() * 1e3);
        people.draw(at.drawn, 0.0, at.ground, &mut scene);
        renderer.render_offscreen(creeer_render::Frame {
            camera: at.camera,
            scene: &scene,
            terrain: &view.draw,
            terrain_frames: &[],
            lighting: at.lighting,
            lines: &creeer_render::DebugLines::default(),
            sky: Some(at.sky),
            heatmap: None,
            overlay: None,
            clear_color: crate::BACKGROUND,
        });
        stats.push(started.elapsed());
    }
    let (p, s) = (|q| stats.percentile_ms(q).unwrap_or(0.0), renderer.stats());
    let gpu: Vec<String> = (renderer.gpu_times().iter())
        .map(|t| format!("{} {:.2}", t.name, t.ms))
        .collect();
    println!(
        "{n} frames: p50 {:.2} ms, p99 {:.2} ms; {} things, {} masses of foliage; {} drawn, \
         {} in shadows; CPU (best) snapshot {told:.2} ms, scene {built:.2} ms; GPU ms: {}",
        p(50.0),
        p(99.0),
        counts.0,
        counts.1,
        s.instances_drawn,
        s.shadow_instances,
        gpu.join(", ")
    );
}
