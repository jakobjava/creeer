//! The planet game's screen, drawn over the world each frame: one function the game and the
//! headless frames (`hud_shot.rs`) both call, so a frame shows what the player sees.

use crate::bindings::Bindings;
use crate::planet_ui::{self, Request};
use crate::planet_view::PlanetView;
use crate::profiler_ui::ProfilerWindow;
use creeer_render::Camera;
use creeer_ui::{PerfInfo, egui, perf_window};

/// Draws the planet game's screen; returns what the app should do.
pub(crate) fn draw(
    ctx: &egui::Context,
    planet: &mut PlanetView,
    camera: &Camera,
    (perf, profiler): (&PerfInfo<'_>, &mut ProfilerWindow),
    bindings: &Bindings,
) -> Request {
    perf_window(ctx, perf);
    let request = planet_ui::window(ctx, planet, camera);
    crate::inventory_ui::window(ctx, planet, camera);
    let target = crate::planet_aim::target(planet, camera);
    crate::hud_feedback::draw(ctx, &planet.session.connection);
    let (seen, name) = crate::hud_seen::seen(planet, target.as_ref());
    let prompts = crate::prompts::prompts(&seen);
    crate::hud_crosshair::draw(ctx, name.as_deref(), &prompts, bindings);
    crate::inventory_ui::told(ctx, planet);
    crate::inventory_ui::vitals(ctx, &planet.session.connection);
    profiler.window(ctx, Some(&mut planet.session.connection));
    request
}
