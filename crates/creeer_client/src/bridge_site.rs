//! The bridge site as a game (G2.11, ADR-0037): `creeer_client --bridge`, or "Bridge site" in
//! the Sandbox window. A creative singleplayer game whose world is made ready at its start
//! (`creeer_server`'s `world/site.rs`): a gully dug beside the spawn, a footbridge's timber and
//! its loads laid a few steps away, the player on the near bank looking across, walking, in the
//! morning.

use crate::planet_view::PlanetView;
use creeer_render::Camera;
use creeer_server::ServerWorld;
use creeer_server::world::load_terrain;
use creeer_server::world::site::BridgeSite;

/// The sun's height when the game starts (°): mid-morning, long shadows across the gully.
const SUN: f64 = 35.0;

/// A creative game on the reference planet with the bridge site made at its spawn.
///
/// # Errors
/// If the planet cannot be loaded.
pub fn view() -> Result<(PlanetView, BridgeSite), String> {
    let data = crate::content::data();
    let planet = crate::planet_net::PLANET;
    let mut world = ServerWorld::new(
        std::sync::Arc::clone(&data),
        load_terrain(&data, planet)?,
        planet,
    );
    world.creative = true;
    let site = world.prepare_bridge_site();
    hosted(world, site)
}

/// The same with the *bridge_demo* scene's footbridge built across the gully (its frames).
///
/// # Errors
/// If the planet cannot be loaded.
pub fn view_built() -> Result<(PlanetView, BridgeSite), String> {
    let data = crate::content::data();
    let terrain = load_terrain(&data, crate::planet_net::PLANET)?;
    let world = creeer_server::play::bridge_demo::bridge_demo_world(terrain);
    let site = BridgeSite::of(&world, 1);
    hosted(world, site)
}

/// `world` hosted in this process, the player walking in first person, mid-morning.
fn hosted(world: ServerWorld, site: BridgeSite) -> Result<(PlanetView, BridgeSite), String> {
    let mut view = PlanetView::hosting(world)?;
    view.walking = true;
    view.first_person = true;
    view.day_fraction = crate::planet_shot::day_for_sun(&view, site.stand.normalize(), SUN);
    Ok((view, site))
}

/// The camera at the player's eyes where they start, looking across the gully and a little
/// down at its far rim.
pub fn camera(site: &BridgeSite) -> Camera {
    let up = site.stand.normalize();
    let eye = creeer_sim::reach::eye(site.stand + up * 0.9, up);
    let rim = site.middle + site.across * (creeer_server::world::site::WIDTH * 0.5);
    crate::planet_camera::place(eye, (rim - eye).normalize() + up * 0.05)
}
