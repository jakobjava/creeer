//! Frames of the player's own cameras (G2.11, A4; ADR-0042), for reviewing what the player
//! sees: `creeer_client --planet <third-crates|first-down> <file.png>`.
//!
//! - `third-crates`: third person (F5), crates stacked a step behind the player: the camera's
//!   boom is pulled in before them;
//! - `first-down`: first person, the player knocked down by a crate dropped on them: the view
//!   from the head where it lies.
//!
//! (`avatar`, in `planet_shot.rs`, is third person in the open.)

use crate::people::DrawnPerson;
use crate::planet_shot_build::play;
use crate::planet_view::PlanetView;
use creeer_math::glam::DVec3;
use creeer_net::message::Command;
use creeer_render::Camera;

/// The views this module takes.
pub(crate) const VIEWS: [&str; 2] = ["third-crates", "first-down"];

/// The camera of view `name` (one of [`VIEWS`]), the scene built and played to it.
///
/// # Errors
/// If the player has not come.
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
    let looking = |view: &PlanetView, ahead: DVec3| {
        let mut camera = view.ground_camera(up, 2.0, ahead, 15f64.to_radians());
        camera.position = me;
        camera
    };
    if name == "third-crates" {
        // Three crates a step behind (west of) the player, one on another.
        for (k, side) in [(0.0, -0.6), (0.0, 0.6), (1.0, 0.0)] {
            let at = ground(me - east * 1.6 + north * side) + up * (0.6 + 1.1 * k);
            view.send(Command::Crate(at));
            play(view, 1.0)?;
        }
        view.first_person = false;
        let mut camera = looking(view, east);
        view.follow(&mut camera, 1.0);
        return Ok(camera);
    }
    // A crate dropped on the player from 3 m: down, looked at from the head where it lies.
    view.send(Command::Crate(ground(me) + up * 3.5));
    play(view, 3.0)?;
    if !view.session.connection.fallen {
        return Err("the player was not knocked down".into());
    }
    view.first_person = true;
    let mut camera = looking(view, north);
    view.follow(&mut camera, 1.0);
    Ok(camera)
}

/// The people drawn in view `name`: in a first-person view, all but the player's own.
pub(crate) fn people(view: &PlanetView, name: &str, people: &mut Vec<DrawnPerson>) {
    if name != "first-down" {
        return;
    }
    let own = (view.session.connection.welcome.as_ref()).map(|w| w.avatar.0);
    people.retain(|p| Some(p.key) != own);
}
