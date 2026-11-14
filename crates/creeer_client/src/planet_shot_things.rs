//! Frames of the things a player handles and stands among (G2.11 A2, ADR-0037), for comparing
//! how they look before and after a change: `creeer_client --planet <view> <file.png>`.
//!
//! - `parts`: every part of the creative kit dropped in a row on the ground before the player
//!   and seen from their eyes, 1.5–3 m off (the rods, the sill, a stick, stones, a crate);
//! - `nailed`: two green spruce poles laid across each other (the *builders* scene's), nailed
//!   where they cross by the player, seen from their eyes;
//! - `forest`: from eye height 40 m into the forest beyond the spawn's clearing;
//! - `tree`: the nearest tree there at least 25 cm thick, from 8 m off at eye height.

use crate::planet_view::PlanetView;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::EntityId;
use creeer_net::message::{Command, Target};
use creeer_render::Camera;
use std::time::Duration;

/// The views this module takes.
pub(crate) const VIEWS: [&str; 4] = ["parts", "nailed", "forest", "tree"];

/// The world a view is played in: the builders' poles for `nailed`, else the creative
/// singleplayer game.
pub(crate) fn view(name: &str) -> Result<PlanetView, String> {
    if name != "nailed" {
        return PlanetView::singleplayer(true);
    }
    let data = crate::content::data();
    let terrain = creeer_server::world::load_terrain(&data, crate::planet_net::PLANET)?;
    let scene = creeer_server::play::scenes::scene("builders").ok_or("no builders scene")?;
    PlanetView::hosting((scene.build)(terrain))
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
        let at = view.session.connection.avatar().ok_or("no avatar")?.position;
        let up = at.normalize();
        let off = (crossing - at).reject_from(up);
        if off.length() < 1.1 {
            break;
        }
        view.session
            .update(step.as_secs_f64(), (off.normalize() * 1.2, false, DVec3::ZERO));
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
    let c = &view.session.connection;
    eprintln!(
        "poles {a} {b}; aimed {:?}; refused {:?}; noted {:?}; kinds {:?}",
        aimed.body,
        c.refused.as_ref().map(|r| &r.0),
        c.noted.as_ref().map(|r| &r.0),
        (c.kinds.iter())
            .map(|(id, k)| (id.0, std::mem::discriminant(k)))
            .collect::<Vec<_>>()
    );
    let me = (view.session.connection.avatar())
        .ok_or("no avatar")?
        .position;
    let eye = creeer_sim::reach::eye(me, up);
    Ok(crate::planet_view::place(eye, (crossing - eye).normalize()))
}
