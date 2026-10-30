//! First-person frames of building (G2.10, ADR-0036's first-person placing), for reviewing
//! what the player sees: `creeer_client --planet <post|rail> <file.png>`.
//!
//! - `post`: a post from the creative kit taken, stood up (R) and aimed at a hole dug 1.5 m
//!   before the player: held over it, let down to its floor;
//! - `rail`: a rail taken and aimed at a squared sill's top 1.5 m before the player: laid on it.
//!
//! As the game plays them: the hold placed each step as `holding.rs` places it (the crosshair
//! where the camera looks), the player's own body not drawn.

use crate::people::DrawnPerson;
use crate::planet_view::PlanetView;
use creeer_math::glam::DVec3;
use creeer_net::message::{Command, Target};
use creeer_render::Camera;
use creeer_terrain::edits::{Brush, Edit};
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

/// The views this module takes.
pub(crate) const VIEWS: [&str; 2] = ["post", "rail"];

/// How long the hands are given to bring what they hold where it is aimed (s).
const SETTLE: f64 = 6.0;

/// The kit's item named `name` (its index, as `Command::Kit` takes it).
fn kit_item(name: &str) -> Result<u8, String> {
    let data = creeer_data::Data::base();
    let kit = creeer_sim::items::kit::kit(&data)?;
    let k = (kit.iter().position(|k| k.name == name)).ok_or(format!("no {name} in the kit"))?;
    u8::try_from(k).map_err(|e| e.to_string())
}

/// The camera at the player's eyes (as the game puts it in first person) looking at `target`.
fn looking_at(view: &PlanetView, target: DVec3) -> Option<Camera> {
    let me = view.session.connection.avatar()?.position;
    let eye = creeer_sim::reach::eye(me, me.normalize());
    Some(crate::planet_camera::place(eye, (target - eye).normalize()))
}

/// Runs the game `seconds`, looking at `target` from the eyes and placing what the hands hold
/// each step as the game does. Returns the camera last looked from.
fn hold(view: &mut PlanetView, target: DVec3, seconds: f64) -> Result<Camera, String> {
    let step = Duration::from_millis(10);
    let mut left = seconds;
    loop {
        let camera = looking_at(view, target).ok_or("no avatar")?;
        if left <= 0.0 {
            return Ok(camera);
        }
        let c = &view.session.connection;
        let surface = c.surface_aimed(camera.rotation(), |_, _, _| None);
        view.holding
            .update(&camera, &mut view.session.connection, surface);
        view.session
            .update(step.as_secs_f64(), (DVec3::ZERO, false, DVec3::ZERO));
        std::thread::sleep(step);
        left -= step.as_secs_f64();
    }
}

/// The camera of view `name` (one of [`VIEWS`]), the scene built and played to it.
///
/// # Errors
/// If the player has not come, or what is placed cannot be taken.
pub(crate) fn camera(view: &mut PlanetView, name: &str) -> Result<Camera, String> {
    play(view, 2.0)?;
    let me = (view.session.connection.avatar())
        .ok_or("no avatar")?
        .position;
    let up = me.normalize();
    let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
    let terrain = std::sync::Arc::clone(&view.terrain);
    let ground =
        |p: DVec3| p.normalize() * (terrain.planet().datum.radius(p) + terrain.surface_height(p));
    let aimed_at = ground(me + east * 1.5);
    let (item, lying) = if name == "post" {
        view.session.edit(Edit::Dig(Brush::Capsule {
            a: aimed_at,
            b: aimed_at - up * 0.6,
            radius: 0.15,
        }));
        ("post", false)
    } else {
        let sill = kit_item("sill")?;
        view.send(Command::Kit {
            item: sill,
            at: aimed_at + up * 0.2,
        });
        ("rail", true)
    };
    // The part to take, a metre north, lying.
    let at = ground(me + north * 1.0) + up * 0.15;
    view.send(Command::Kit {
        item: kit_item(item)?,
        at,
    });
    play(view, 1.5)?;
    let c = &view.session.connection;
    // Where it lies as drawn: the body nearest where it was dropped (it falls and rolls).
    let own = c.welcome.as_ref().map(|w| w.avatar);
    let lies = (c.poses().into_iter())
        .filter(|(id, _)| Some(*id) != own)
        .map(|(_, p)| p.position)
        .min_by(|a, b| (*a - at).length().total_cmp(&(*b - at).length()))
        .ok_or("the part is not there")?;
    let taken = (c.aim(lies + up * 2.0, -up, 3.0)).ok_or("the part is not there")?;
    view.send(Command::Grab(Target {
        body: taken.body,
        at: taken.point,
        seen: taken.seen,
    }));
    play(view, 0.5)?;
    // Looking at where it goes: on the sill's top, or the hole's mouth.
    let target = if lying {
        aimed_at + up * 0.15
    } else {
        aimed_at
    };
    hold(view, target, 0.1)?;
    if !view.holding.holding() {
        let c = &view.session.connection;
        let told = (c.refused.as_ref().map(|r| r.0.clone()), taken.body);
        return Err(format!("the part was not taken: {told:?}"));
    }
    if !lying {
        view.holding.key(KeyCode::KeyR, true);
    }
    hold(view, target, SETTLE)
}

/// The people drawn in view `name`: in a build view (first person), all but the player's own.
pub(crate) fn people(view: &PlanetView, name: &str, people: &[DrawnPerson]) -> Vec<DrawnPerson> {
    let own = (view.session.connection.welcome.as_ref()).map(|w| w.avatar.0);
    (people.iter())
        .filter(|p| !(VIEWS.contains(&name) && Some(p.key) == own))
        .cloned()
        .collect()
}
/// Runs the game for `seconds` of real time (the hosted server steps on its own; the client
/// takes in what it is told), first until the client has joined.
pub(crate) fn play(view: &mut PlanetView, seconds: f64) -> Result<(), String> {
    let start = Instant::now();
    let step = Duration::from_millis(10);
    while view.session.connection.status() != &creeer_server::client::Status::Joined {
        if start.elapsed() > Duration::from_secs(10) {
            return Err("the hosted server did not welcome the client".into());
        }
        view.session
            .update(step.as_secs_f64(), (DVec3::ZERO, false, DVec3::ZERO));
        std::thread::sleep(step);
    }
    let start = Instant::now();
    while start.elapsed().as_secs_f64() < seconds {
        view.session
            .update(step.as_secs_f64(), (DVec3::ZERO, false, DVec3::ZERO));
        std::thread::sleep(step);
    }
    Ok(())
}
