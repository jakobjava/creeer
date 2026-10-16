//! The planet sandbox mode of the app (F3): enter, leave, and draw a frame.

use super::*;
use crate::planet_ui::{self, Request};
use crate::planet_view::{PlanetView, load_terrain};

impl Running {
    /// Opens the planet sandbox, starting in orbit.
    pub(super) fn enter_planet(&mut self) {
        match load_terrain() {
            Ok(terrain) => {
                let view = PlanetView::new(terrain, TICK_RATE_HZ);
                self.camera = view.orbit_camera();
                self.planet = Some(view);
                self.lighting_key = None;
            }
            Err(e) => eprintln!("could not load the planet: {e}"),
        }
    }

    /// Back to the tech demo.
    fn leave_planet(&mut self) {
        self.planet = None;
        self.camera = demo::camera_preset("overview", self.scene_origin()).expect("preset exists");
        self.lighting_key = None;
    }

    /// One frame of the planet sandbox.
    pub(super) fn planet_frame(&mut self, now: Instant, dt: Duration) {
        let dt = dt.as_secs_f64();
        let Some(planet) = &mut self.planet else {
            return;
        };
        // Flying speed follows the height above the ground: the wheel sets how many heights
        // per second (10 m/s base speed = one height per second at 10 m).
        let base = self.fly.speed;
        self.fly.speed = base * planet.altitude(self.camera.position).max(2.0) / 10.0;
        self.fly.update(&mut self.camera, dt);
        self.fly.speed = base;
        planet.constrain_camera(&mut self.camera);
        planet.advance_day(dt);
        planet.update_terrain(&mut self.renderer, self.camera.position);

        let build_start = Instant::now();
        self.scene.clear();
        self.lines.clear();
        let published = planet.snapshot();
        let alpha = published.alpha(now);
        physics_view::draw(
            &mut self.scene,
            &self.meshes,
            DVec3::ZERO,
            (&published.previous.bodies, &published.current.bodies, alpha),
            Overlay::Off,
        );
        self.scene_build_times.push(build_start.elapsed());

        // Lighting: colours from the atmosphere for the sun's elevation here (costly, so only
        // when it changed), the direction every frame.
        let sky = planet.sky();
        let sun = planet.sun_direction();
        let (height, up) = sky.height_and_up(self.camera.position);
        let elevation = creeer_math::det::asin(up.dot(sun).clamp(-1.0, 1.0)).to_degrees();
        let stale = self.lighting_key.is_none_or(|(h, e)| {
            (height - h).abs() > (0.02 * h.abs()).max(1.0) || (elevation - e).abs() > 0.2
        });
        if stale {
            self.lighting = sky.lighting(self.camera.position, sun, &Lighting::default());
            self.lighting_key = Some((height, elevation));
        }
        self.lighting.sun_direction = sun;
        self.lighting.up = up;

        let raw_input = self.egui_state.take_egui_input(&self.window);
        let ctx = self.egui_ctx.clone();
        let tick_times = planet.sim.tick_times();
        let mut request = Request::None;
        let mut output = ctx.run_ui(raw_input, |ui| {
            perf_window(
                ui.ctx(),
                &PerfInfo {
                    frame_times: &self.frame_times,
                    tick_times: &tick_times,
                    tick_rate_hz: TICK_RATE_HZ,
                    tick: published.tick,
                    adapter: &self.adapter_label,
                },
            );
            request = planet_ui::window(ui.ctx(), planet, &self.camera);
            planet_ui::crosshair(ui.ctx());
        });
        self.egui_state
            .handle_platform_output(&self.window, output.platform_output);
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        self.renderer.render(Frame {
            camera: &self.camera,
            scene: &self.scene,
            terrain: &planet.draw,
            terrain_frames: &[],
            lighting: &self.lighting,
            sky: Some(&sky),
            heatmap: None,
            lines: &self.lines,
            overlay: Some(OverlayFrame {
                primitives: &primitives,
                textures_delta: &mut output.textures_delta,
                pixels_per_point: output.pixels_per_point,
            }),
            clear_color: BACKGROUND,
        });
        match request {
            Request::None => {}
            Request::Leave => self.leave_planet(),
            Request::Orbit => {
                if let Some(planet) = &self.planet {
                    self.camera = planet.orbit_camera();
                }
            }
        }
    }
}
