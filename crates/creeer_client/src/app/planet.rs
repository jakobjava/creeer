//! The planet sandbox mode of the app (F3; a client of a server since F5): enter it (a
//! singleplayer game, or a dedicated server's), leave it, walk or fly, and draw a frame.

use super::*;
use crate::bindings::Action;
use crate::planet_ui::Request;
use crate::planet_view::PlanetView;

/// How the game starts (`main.rs`): the tech demo, a dedicated server joined at once, or the
/// bridge site.
#[derive(Debug, Default)]
pub(crate) enum Start {
    #[default]
    Demo,
    Connect(std::net::SocketAddr, String),
    Bridge,
}

/// Walking speed (m/s), and running.
const WALK: f64 = 3.0;
const RUN: f64 = 6.5;

impl Running {
    /// Opens the planet sandbox, starting in orbit.
    pub(super) fn enter_planet(&mut self) {
        // Survival unless asked for creative (G1.6b: the user, 2026-10-22).
        let creative = std::env::args().any(|a| a == "--creative");
        match PlanetView::singleplayer(creative) {
            Ok(view) => {
                self.camera = view.orbit_camera();
                self.planet = Some(view);
                self.lighting_key = None;
            }
            Err(e) => eprintln!("could not load the planet: {e}"),
        }
    }

    /// Starts as asked (`main.rs`).
    pub(super) fn start(&mut self, start: Start) -> Result<(), String> {
        match start {
            Start::Demo => Ok(()),
            Start::Connect(server, name) => self.enter_connected(server, &name),
            Start::Bridge => self.enter_bridge(),
        }
    }

    /// Opens the bridge site (G2.11, ADR-0039; `bridge_site.rs`): a creative game at a gully, the
    /// player on its near bank looking across.
    pub(super) fn enter_bridge(&mut self) -> Result<(), String> {
        let (view, site) = crate::bridge_site::view()?;
        self.camera = crate::bridge_site::camera(&site);
        self.planet = Some(view);
        self.lighting_key = None;
        Ok(())
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

    /// An action of the planet game (the controls' table, `bindings.rs`): walk or fly; take what
    /// the crosshair is on in both hands, or put down what they hold (G1.4f); put away, put in
    /// and show what is carried (G1.6a, `inventory_ui.rs`); throw, strip bark, bind a head on
    /// (G1.6d); fasten, set a post, take a joint apart, choose a fastener (G2); a tool in hand
    /// (G1.4c), the same again or put away. Returns whether it was one.
    pub(super) fn planet_act(&mut self, action: Action) -> bool {
        let Some(planet) = &mut self.planet else {
            return false;
        };
        let camera = &self.camera;
        let tool = match action {
            Action::WalkOrFly => {
                planet.walking = !planet.walking;
                return true;
            }
            Action::Take => {
                planet.grab_or_drop(camera);
                return true;
            }
            Action::Pocket => {
                planet.stow(camera);
                return true;
            }
            Action::PutIn => {
                planet.put_in_or_open(camera);
                return true;
            }
            Action::Inventory => {
                planet.inventory = !planet.inventory;
                return true;
            }
            Action::Throw => {
                planet.send(creeer_net::message::Command::Throw(camera.forward()));
                return true;
            }
            // First person, or the camera behind to look at the build (G2.10).
            Action::Camera => {
                planet.first_person = !planet.first_person;
                return true;
            }
            Action::Strip => {
                planet.send(creeer_net::message::Command::Strip);
                return true;
            }
            Action::Fasten => {
                planet.fasten(camera);
                return true;
            }
            Action::SetPost => {
                planet.set_post(camera);
                return true;
            }
            Action::Unfasten => {
                planet.unfasten(camera);
                return true;
            }
            Action::NextFastener => {
                planet.fastening.next();
                return true;
            }
            Action::Bind => {
                planet.bind(camera);
                return true;
            }
            Action::Tool1 => Some(0),
            Action::Tool2 => Some(1),
            Action::Tool3 => Some(2),
            Action::Tool4 => Some(3),
            Action::ToolAway => None,
            // The stress overlay, on a creative server this process hosts (G2.11, ADR-0039).
            Action::StressOverlay => {
                planet.stress.on = !planet.stress.on;
                return true;
            }
            _ => return false,
        };
        let tool = if tool == planet.wielding { None } else { tool };
        planet.wielding = tool;
        planet.send(creeer_net::message::Command::Wield { tool });
        true
    }

    /// The steering of what the hands hold, while on the planet (G2.10).
    pub(super) fn holding(&mut self) -> Option<&mut crate::holding::Holding> {
        self.planet.as_mut().map(|p| &mut p.holding)
    }

    /// Back to the tech demo.
    pub(super) fn leave_planet(&mut self) {
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
            // Where the hands hold what they hold, placed where the crosshair meets something
            // (G2.10): sent with the inputs. Standing trees are aimed at too.
            let surface = planet.holding.holding().then(|| {
                let c = &planet.session.connection;
                let trees = planet.trees.as_ref();
                c.surface_aimed(self.camera.rotation(), |from, along, most| {
                    trees.and_then(|t| t.aim(c, from, along, most)).map(|a| a.2)
                })
            });
            planet.holding.update(
                &self.camera,
                &mut planet.session.connection,
                surface.flatten(),
            );
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
        let mut snapshot = planet.snapshot();
        // What a creative server shows of what is built: the overlay draws the structures.
        let stress = planet.stress;
        let seen = (planet.session.inspection(stress.on)).unwrap_or_default();
        stress.leave_out(&mut snapshot.beams);
        physics_view::draw(
            &mut self.scene,
            &self.meshes,
            DVec3::ZERO,
            (&snapshot, &snapshot, 1.0),
            Overlay::Off,
        );
        let c = &planet.session.connection;
        stress.draw(&mut self.scene, &self.meshes, c, &seen);
        if let Some(people) = &mut self.people {
            // In first person the player's own body is seen from within, its head sunk into it
            // (G2.11: their own arms and hands at work; not drawn, the part floated).
            let own = (planet.session.connection.welcome.as_ref()).map(|w| w.avatar.0);
            people.first_person = own.filter(|_| planet.first_person);
            let drawn_people = snapshot.people.clone();
            let ground = crate::people_ground::PlanetGround {
                terrain: &planet.terrain,
            };
            // The made tools in hands as their pieces, as the world's made things are drawn.
            for (c, q, half, [r, g, b]) in people.draw(&drawn_people, dt, &ground, &mut self.scene)
            {
                let colour = [r, g, b].map(|x| f32::from(x) / 255.0);
                let material = creeer_render::Material::dielectric(colour, 0.85);
                physics_view::add_box(
                    &mut self.scene,
                    &self.meshes,
                    DVec3::ZERO,
                    (c, q, half),
                    material,
                    None,
                );
            }
        }
        self.scene_build_times.push(build_start.elapsed());
        // What is heard of this frame (ADR-0037; the people's feet as just drawn).
        let mut sound = std::mem::take(&mut planet.sound);
        sound.frame(planet, &self.camera, self.people.as_ref(), dt);
        planet.sound = sound;

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
        let mut asked = crate::menu::Asked::default();
        let mut output = ctx.run_ui(raw_input, |ui| {
            let perf = PerfInfo {
                frame_times: &self.frame_times,
                tick_times: &tick_times,
                tick_rate_hz: TICK_RATE_HZ,
                tick,
                adapter: &self.adapter_label,
            };
            request = crate::hud::draw(
                ui.ctx(),
                planet,
                &self.camera,
                (&perf, &mut self.profiler),
                (&self.game, &mut self.screen),
            );
            asked = crate::menu::draw(
                ui.ctx(),
                &mut self.screen,
                &mut self.game,
                Some(&mut *planet),
            );
            if let Some(scale) = stress.heatmap() {
                heatmap_legend(ui.ctx(), &scale, "Share of what breaks it", "%");
            }
            crate::stress_view::hud(ui.ctx(), &self.camera, &seen, tick);
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
            heatmap: stress.heatmap().as_ref(),
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
        self.menu_asked(asked);
    }
}
