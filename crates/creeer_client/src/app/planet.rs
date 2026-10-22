//! The planet sandbox mode of the app (F3; a client of a server since F5): enter it (a
//! singleplayer game, or a dedicated server's), leave it, walk or fly, and draw a frame.

use super::*;
use crate::planet_ui::{self, Request};
use crate::planet_view::PlanetView;

/// Walking speed (m/s), and running.
const WALK: f64 = 3.0;
const RUN: f64 = 6.5;

impl Running {
    /// Opens the planet sandbox, starting in orbit.
    pub(super) fn enter_planet(&mut self) {
        match PlanetView::singleplayer() {
            Ok(view) => {
                self.camera = view.orbit_camera();
                self.planet = Some(view);
                self.lighting_key = None;
            }
            Err(e) => eprintln!("could not load the planet: {e}"),
        }
    }

    /// Joins the dedicated server at `server` as `name`, walking from the start.
    pub(super) fn enter_connected(
        &mut self,
        server: std::net::SocketAddr,
        name: &str,
    ) -> Result<(), String> {
        let mut view = PlanetView::connect(server, name)?;
        view.walking = true;
        self.camera = view.orbit_camera();
        self.planet = Some(view);
        self.lighting_key = None;
        Ok(())
    }

    /// A key for the planet sandbox: V walks or flies; 1 to 4 take a tool in hand (G1.4c: the
    /// tools' table's, until G1.6's inventory), the same again or 0 put it away. Returns
    /// whether it was one.
    pub(super) fn planet_key(&mut self, code: winit::keyboard::KeyCode) -> bool {
        use winit::keyboard::KeyCode;
        let Some(planet) = &mut self.planet else {
            return false;
        };
        let tool = match code {
            KeyCode::KeyV => {
                planet.walking = !planet.walking;
                return true;
            }
            KeyCode::Digit1 => Some(0),
            KeyCode::Digit2 => Some(1),
            KeyCode::Digit3 => Some(2),
            KeyCode::Digit4 => Some(3),
            KeyCode::Digit0 => None,
            _ => return false,
        };
        let tool = if tool == planet.wielding { None } else { tool };
        planet.wielding = tool;
        planet.send(creeer_net::message::Command::Wield { tool });
        true
    }

    /// Back to the tech demo.
    fn leave_planet(&mut self) {
        self.planet = None;
        self.camera = demo::camera_preset("overview", self.scene_origin()).expect("preset exists");
        self.lighting_key = None;
    }

    /// One frame of the planet sandbox.
    pub(super) fn planet_frame(&mut self, dt: Duration) {
        let dt = dt.as_secs_f64();
        let Some(planet) = &mut self.planet else {
            return;
        };
        if planet.walking {
            // The keys steer the avatar; the camera follows it.
            self.fly.look(&mut self.camera);
            let speed = if self.fly.running() { RUN } else { WALK };
            let walk = planet.walk(&self.camera, self.fly.walk_axes(), speed);
            let thrust = planet.thrust(walk, self.fly.thrusting());
            planet
                .session
                .update(dt, (walk, self.fly.jumping(), thrust));
            planet.follow(&mut self.camera);
        } else {
            // Flying speed follows the height above the ground: the wheel sets how many
            // heights per second (10 m/s base speed = one height per second at 10 m).
            let base = self.fly.speed;
            self.fly.speed = base * planet.altitude(self.camera.position).max(2.0) / 10.0;
            self.fly.update(&mut self.camera, dt);
            self.fly.speed = base;
            planet.session.update(dt, (DVec3::ZERO, false, DVec3::ZERO));
            planet.constrain_camera(&mut self.camera);
        }
        planet.advance_day(dt);
        planet.update_terrain(&mut self.renderer, self.camera.position);

        let build_start = Instant::now();
        self.scene.clear();
        self.lines.clear();
        // Drawn where the connection has everything now (it interpolates).
        let snapshot = planet.snapshot();
        physics_view::draw(
            &mut self.scene,
            &self.meshes,
            DVec3::ZERO,
            (&snapshot, &snapshot, 1.0),
            Overlay::Off,
        );
        if let Some(people) = &mut self.people {
            let ground = crate::people_ground::PlanetGround {
                terrain: &planet.terrain,
            };
            people.draw(&snapshot.people, dt, &ground, &mut self.scene);
        }
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
        // The server's ticks when this process hosts it.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // ticks
        let (tick_times, tick) = planet.session.tick_times().unwrap_or_else(|| {
            (
                TimingStats::new(1),
                planet.session.connection.drawn_at().max(0.0) as u64,
            )
        });
        let mut request = Request::None;
        let mut output = ctx.run_ui(raw_input, |ui| {
            perf_window(
                ui.ctx(),
                &PerfInfo {
                    frame_times: &self.frame_times,
                    tick_times: &tick_times,
                    tick_rate_hz: TICK_RATE_HZ,
                    tick,
                    adapter: &self.adapter_label,
                },
            );
            request = planet_ui::window(ui.ctx(), planet, &self.camera);
            planet_ui::crosshair(ui.ctx());
            self.profiler
                .window(ui.ctx(), Some(&mut planet.session.connection));
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
