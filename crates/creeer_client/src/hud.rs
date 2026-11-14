//! The planet game's screen, drawn over the world each frame: one function the game and the
//! headless frames (`hud_shot.rs`) both call, so a frame shows what the player sees (G2.11 A3,
//! ADR-0038). By default only what play needs: the crosshair and what it tells, what came of
//! what was done, the hands, the vitals. The debug view (F3) adds the performance and the
//! planet's numbers; the menu (Esc) dims it all.

use crate::bindings::Action;
use crate::menu::Screen;
use crate::planet_ui::{self, Request};
use crate::planet_view::PlanetView;
use crate::profiler_ui::ProfilerWindow;
use crate::settings::GameSettings;
use creeer_render::Camera;
use creeer_ui::paint::{self, Row};
use creeer_ui::theme::{BODY, TEXT};
use creeer_ui::{PerfInfo, egui, perf_window};

/// Draws the planet game's screen; returns what the app should do.
pub(crate) fn draw(
    ctx: &egui::Context,
    planet: &mut PlanetView,
    camera: &Camera,
    (perf, profiler): (&PerfInfo<'_>, &mut ProfilerWindow),
    (game, screen): (&GameSettings, &mut Screen),
) -> Request {
    let mut request = Request::None;
    if screen.debug {
        perf_window(ctx, perf);
        request = planet_ui::window(ctx, planet, camera);
    }
    crate::inventory_ui::window(ctx, planet, camera, &game.bindings);
    if screen.kit {
        crate::kit_ui::window(ctx, &mut screen.kit, planet, camera);
    }
    if screen.menu.is_none() {
        let target = crate::planet_aim::target(planet, camera);
        crate::hud_feedback::draw(ctx, &planet.session.connection);
        let (seen, name) = crate::hud_seen::seen(planet, target.as_ref());
        let prompts = crate::prompts::prompts(&seen);
        crate::hud_crosshair::draw(ctx, name.as_deref(), &prompts, &game.bindings);
        let tool = planet.wielding.and_then(tool_name);
        let held = crate::hud_seen::held_name(planet);
        let inventory = &planet.session.connection.inventory;
        let hands = (inventory, held.as_deref());
        crate::hud_hands::draw(ctx, hands, tool.as_deref(), &game.bindings);
    }
    crate::inventory_ui::vitals(ctx, &planet.session.connection);
    if !game.controls_seen && screen.menu.is_none() {
        first_hint(ctx, game);
    }
    profiler.window(ctx, Some(&mut planet.session.connection));
    request
}

/// The name of tool `index` of the tools' table (G1.4c), as a person says it.
fn tool_name(index: u8) -> Option<String> {
    static NAMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let names = NAMES.get_or_init(|| {
        (creeer_sim::tools::Tool::all(&crate::content::data()).unwrap_or_default())
            .into_iter()
            .map(|t| t.name.replace('_', " "))
            .collect()
    });
    names.get(usize::from(index)).cloned()
}

/// Until the player has opened the controls once: how to, at the top of the screen.
fn first_hint(ctx: &egui::Context, game: &GameSettings) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("first hint"),
    ));
    let key = game.bindings.name(Action::Menu);
    let row = Row {
        key: Some(&key),
        text: "Menu: the controls and settings",
        colour: TEXT,
        size: BODY,
    };
    let top = ctx.content_rect().center_top() + egui::vec2(0.0, 56.0);
    paint::strip(&painter, top, &[row]);
}
