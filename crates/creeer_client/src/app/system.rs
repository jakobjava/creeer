//! The solar-system sandbox mode of the app (F4): enter, leave, input and a frame.

use super::*;
use crate::system_cmd::Command;
use crate::system_frame;
use crate::system_ui::{self, Request, WARPS};
use crate::system_view::SystemView;
use winit::keyboard::KeyCode;

/// Generates the default system and builds its universe.
pub fn load_universe() -> Result<creeer_sim::Universe, String> {
    let data = creeer_terrain::data_path("");
    let system = creeer_orbit::generate::generate_file(&data.join("systems/default.ron"), &data)
        .map_err(|e| e.to_string())?;
    Ok(creeer_sim::Universe::new(
        system,
        data,
        1.0 / f64::from(TICK_RATE_HZ),
    ))
}

impl Running {
    /// Opens the solar-system sandbox, the camera above home.
    pub(super) fn enter_system(&mut self) {
        match load_universe() {
            Ok(universe) => {
                let radius = universe.bodies.ephemeris.bodies()[universe.home].radius;
                let view = SystemView::new(universe, TICK_RATE_HZ);
                let up = DVec3::new(0.3, 0.9, 0.2).normalize();
                self.camera = crate::planet_view::place(up * (radius + 3000.0), up.cross(DVec3::X));
                self.system = Some(view);
                self.lighting_key = None;
            }
            Err(e) => eprintln!("could not create the solar system: {e}"),
        }
    }

    fn leave_system(&mut self) {
        self.system = None;
        self.camera = demo::camera_preset("overview", self.scene_origin()).expect("preset exists");
        self.lighting_key = None;
    }

    /// A key pressed in the sandbox; returns whether it was used.
    pub(super) fn system_key(&mut self, code: KeyCode) -> bool {
        let Some(view) = &mut self.system else {
            return false;
        };
        let warp = view.snapshot().current.requested_warp;
        match code {
            KeyCode::KeyM => view.map = !view.map,
            KeyCode::Period => {
                if let Some(&w) = WARPS.iter().find(|&&w| w > warp) {
                    view.send(Command::Warp(w));
                }
            }
            KeyCode::Comma => {
                if let Some(&w) = WARPS.iter().rev().find(|&&w| w < warp) {
                    view.send(Command::Warp(w));
                }
            }
            KeyCode::KeyB => {
                view.engine.on = !view.engine.on;
                view.send(Command::Engine(view.engine));
            }
            _ => return false,
        }
        true
    }

    /// One frame of the solar-system sandbox.
    pub(super) fn system_frame(&mut self, now: Instant, dt: Duration) {
        let Some(view) = &mut self.system else {
            return;
        };
        let published = view.snapshot();
        let s = Arc::clone(&published.current);
        let alpha = published.alpha(now);
        if !view.map {
            view.update_camera(&mut self.camera, &s);
            // Fly at a speed set by the height above the nearest surface.
            let near = system_frame::nearest_surface(&s, self.camera.position);
            let height = near.map_or(1e3, |(_, h)| h);
            let base = self.fly.speed;
            self.fly.speed = base * height.max(2.0) / 10.0;
            let before = self.camera.position;
            self.fly.update(&mut self.camera, dt.as_secs_f64());
            self.fly.speed = base;
            if view.follow.is_some() {
                // Following: the mouse turns the view, the ship sets the position.
                self.camera.position = before;
            } else {
                view.send(Command::FocusPoint(self.camera.position));
            }
            if let Some((centre, _)) = near {
                self.camera
                    .level_to((self.camera.position - centre).normalize());
            }
        }
        let built = system_frame::build(
            view,
            &mut self.renderer,
            (&mut self.scene, &mut self.lines, &self.meshes),
            &self.camera,
            &published,
            alpha,
        );
        // Lighting: the sun where the star is; the sky's colours near home.
        if let Some(sky) = &built.sky {
            let (height, up) = sky.height_and_up(built.camera.position);
            let elevation = creeer_math::det::asin(up.dot(built.sun).clamp(-1.0, 1.0)).to_degrees();
            let stale = self.lighting_key.is_none_or(|(h, e)| {
                (height - h).abs() > (0.02 * h.abs()).max(1.0) || (elevation - e).abs() > 0.2
            });
            if stale {
                self.lighting =
                    sky.lighting(built.camera.position, built.sun, &Lighting::default());
                self.lighting_key = Some((height, elevation));
            }
            self.lighting.up = up;
        } else {
            self.lighting = system_frame::space_lighting();
            self.lighting_key = None;
        }
        self.lighting.sun_direction = built.sun;

        let raw_input = self.egui_state.take_egui_input(&self.window);
        let ctx = self.egui_ctx.clone();
        let tick_times = view.sim.tick_times();
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
            request = system_ui::window(ui.ctx(), view, &s, &built.camera);
            if view.map {
                system_ui::paint_labels(ui.ctx(), &built.camera, &built.labels);
            }
        });
        self.egui_state
            .handle_platform_output(&self.window, output.platform_output);
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let (draw, frames): (&[(u64, u8)], &[creeer_render::TerrainFrame]) = if view.map {
            (&[], &[])
        } else {
            (&view.draw, &view.terrain_frames)
        };
        self.renderer.render(Frame {
            camera: &built.camera,
            scene: &self.scene,
            terrain: draw,
            terrain_frames: frames,
            lighting: &self.lighting,
            sky: built.sky.as_ref(),
            heatmap: None,
            lines: &self.lines,
            overlay: Some(OverlayFrame {
                primitives: &primitives,
                textures_delta: &mut output.textures_delta,
                pixels_per_point: output.pixels_per_point,
            }),
            clear_color: BACKGROUND,
        });
        if request == Request::Leave {
            self.leave_system();
        }
    }

    /// Mouse motion on the map turns its camera.
    pub(super) fn system_mouse(&mut self, dx: f64, dy: f64) -> bool {
        let Some(view) = &mut self.system else {
            return false;
        };
        if !view.map {
            return false;
        }
        view.map_camera.yaw -= dx * 0.005;
        view.map_camera.pitch = (view.map_camera.pitch + dy * 0.005).clamp(-1.5, 1.5);
        true
    }

    /// The wheel zooms the map.
    pub(super) fn system_scroll(&mut self, notches: f64) -> bool {
        let Some(view) = &mut self.system else {
            return false;
        };
        if !view.map {
            if let Some(offset) = &mut view.follow {
                *offset *= creeer_math::det::pow(0.85, notches);
                return true;
            }
            return false;
        }
        view.map_camera.distance *= creeer_math::det::pow(0.8, notches);
        true
    }
}
