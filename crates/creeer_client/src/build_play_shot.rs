//! The frames `build_play.rs` saves (G2.11 B): what the player sees at a step of a build, in
//! first person with the crosshair, and from behind with the own body drawn; and a line of what
//! the player was told, what is held and where she stands.

use crate::build_play::{Player, STEP, UNTIL};
use crate::physics_view;
use creeer_core::TimingStats;
use creeer_math::glam::DVec3;
use creeer_render::{Camera, DebugLines, Frame, Lighting, OverlayFrame, Scene};
use creeer_server::world::site::Inspection;
use creeer_ui::{PerfInfo, egui};
use std::fmt::Write as _;
use std::time::{Duration, Instant};

impl Player {
    /// What is held as drawn: its centre, its long axis's angle from level (°), its centre's
    /// distance from where the crosshair meets something (m).
    fn held_now(&self) -> Option<(DVec3, f64, f64)> {
        let c = &self.view.session.connection;
        let load = c.own_load()?;
        let pose = c.poses().get(&load).copied()?;
        let up = self.up();
        // A rod's long axis is its x.
        let axis = pose.orientation * DVec3::X;
        let tilt = creeer_math::det::asin(axis.dot(up).abs().min(1.0)).to_degrees();
        let hit = c.aim_skipping(self.eye(), self.camera.forward(), 4.0, Some(load));
        let off = hit.map_or(f64::NAN, |h| (pose.position - h.point).length());
        Some((pose.position, tilt, off))
    }

    /// Saves the frame as the player sees it now, named `step`, and logs it.
    ///
    /// # Errors
    /// If the frame cannot be saved.
    pub(crate) fn shot(&mut self, step: &str) -> Result<(), String> {
        if self.shots >= self.until {
            return Err(UNTIL.into());
        }
        self.shot_from(step, self.camera, false)?;
        if self.hud {
            // (The play-through: what the player's screen shows, nothing else.)
            return Ok(());
        }
        // And from behind (F5), the own body drawn: where the part and the body are.
        let way = self.camera.forward();
        let up = self.up();
        let level = way.reject_from(up).try_normalize().unwrap_or(self.east);
        let from = self.me() + up * 1.6 - level * 3.5 + level.cross(up) * 1.2;
        let behind = crate::planet_view::place(from, (self.eye() + level * 1.5 - from).normalize());
        self.shot_from(&format!("{step}-behind"), behind, true)
    }

    /// Saves the frame `camera` sees, named `step` (`own`: the player's own body drawn).
    fn shot_from(&mut self, step: &str, camera: Camera, own: bool) -> Result<(), String> {
        let start = Instant::now();
        loop {
            self.view
                .update_terrain(&mut self.renderer, camera.position);
            let settled = self.view.streaming().1 == 0;
            if settled && start.elapsed() > Duration::from_millis(100) {
                break;
            }
            if start.elapsed() > Duration::from_secs(60) {
                return Err("terrain streaming did not settle".into());
            }
            // The game goes on while the terrain streams (as it does on screen).
            self.frame(DVec3::ZERO);
            std::thread::sleep(Duration::from_secs_f64(STEP));
        }
        let sky = self.view.sky();
        let sun = self.view.sun_direction();
        let mut lighting = sky.lighting(camera.position, sun, &Lighting::default());
        lighting.shadow_distance = 100.0;
        let mut scene = Scene::default();
        let mut snapshot = self.view.snapshot();
        // What a creative server shows of what is built, as the game draws it (`app/planet.rs`).
        let stress = self.view.stress;
        let seen = self.view.session.inspection(stress.on);
        self.screen.creative = seen.is_some();
        let seen = seen.unwrap_or_default();
        stress.leave_out(&mut snapshot.beams);
        let pair = (&snapshot, &snapshot, 1.0);
        physics_view::draw(
            &mut scene,
            &self.meshes,
            DVec3::ZERO,
            pair,
            crate::demo::Overlay::Off,
        );
        let c = &self.view.session.connection;
        stress.draw(&mut scene, &self.meshes, c, &seen);
        // Everyone, the player too: in first person seen from within (`People::first_person`).
        let drawn = snapshot.people.clone();
        let own_key = (self.view.session.connection.welcome.as_ref()).map(|w| w.avatar.0);
        self.people.first_person = own_key.filter(|_| !own);
        let ground = crate::people_ground::PlanetGround {
            terrain: &self.view.terrain,
        };
        let dt = self.last_shot.elapsed().as_secs_f64();
        self.last_shot = Instant::now();
        // The body's animation is given the time since the last frame drawn, at least the time
        // the arms take to reach what the hands hold (frames are seconds apart).
        let mut scratch = Scene::default();
        // (Played at the site, the people are animated every frame: `Player::hear`.)
        let settle = if self.hud { 0.0 } else { dt.max(1.0) };
        self.people.draw(&drawn, settle, &ground, &mut scratch);
        self.people.draw(&drawn, 0.0, &ground, &mut scene);
        // The game's screen over it, as the player sees it (the site), first person only.
        let tick = self.view.session.tick_times().map_or(0, |t| t.1);
        let mut screen = (self.hud && !own).then(|| self.screen_drawn(&camera, &seen, tick));
        let mut image = self.renderer.capture(Frame {
            camera: &camera,
            scene: &scene,
            terrain: &self.view.draw,
            terrain_frames: &[],
            lighting: &lighting,
            lines: &DebugLines::default(),
            sky: Some(&sky),
            heatmap: stress.heatmap().as_ref(),
            overlay: screen.as_mut().map(|s| OverlayFrame {
                primitives: &s.0,
                textures_delta: &mut s.1,
                pixels_per_point: s.2,
            }),
            clear_color: crate::BACKGROUND,
        });
        if !own && !self.hud {
            crosshair(&mut image);
        }
        let n = if own {
            self.shots.saturating_sub(1)
        } else {
            self.shots
        };
        let name = format!("{n:02}-{step}.png");
        if !own {
            self.shots += 1;
        }
        (image.save_png(&self.dir.join(&name))).map_err(|e| e.to_string())?;
        self.shot_taken = true;
        if own {
            return Ok(());
        }
        let c = &self.view.session.connection;
        let told = |m: &Option<(String, Instant)>| {
            m.as_ref()
                .filter(|(_, at)| at.elapsed() < Duration::from_secs(3))
                .map_or(String::new(), |(s, _)| s.clone())
        };
        let held = self
            .held_now()
            .map_or("nothing held".into(), |(at, tilt, off)| {
                // From the eyes: ahead (level), right, up.
                let (eye, up) = (self.eye(), self.up());
                let ahead = self.camera.forward().reject_from(up).normalize_or_zero();
                let d = at - eye;
                format!(
                    "held: {tilt:.1}° from level, centre {off:.2} m from the aim, from the eyes \
                 {:.2} ahead {:.2} right {:.2} up",
                    d.dot(ahead),
                    d.dot(ahead.cross(up)),
                    d.dot(up)
                )
            });
        let target = crate::planet_aim::target(&self.view, &camera).map_or(
            "crosshair on nothing".into(),
            |t| {
                if t.in_reach {
                    format!("{} · {:.1} m", t.what, t.from_hands)
                } else {
                    format!("{} · out of reach", t.what)
                }
            },
        );
        // Where she stands: east and north of the start, her centre over the ground below it.
        let (me, up) = (self.me(), self.up());
        let off = me - self.start;
        let ground = self.ground_at(me, 0.0, 0.0);
        let fallen = c.welcome.as_ref().is_some_and(|w| {
            matches!(
                c.kinds.get(&w.avatar),
                Some(creeer_net::entity::Kind::Character { fallen: true, .. })
            )
        });
        let mut line = format!(
            "{name}: {held}; {target}; at {:.2} E {:.2} N, {:.2} m up{}; noted: {}; refused: {}",
            off.dot(self.east),
            off.dot(self.north),
            (me - ground).dot(up),
            if fallen { ", FALLEN" } else { "" },
            told(&c.noted),
            told(&c.refused)
        );
        if self.hud {
            line.push_str(&self.screen_said(&camera));
        }
        println!("{line}");
        let _ = writeln!(self.log, "{line}");
        let _ = std::fs::write(self.dir.join("steps.txt"), &self.log);
        Ok(())
    }
}

/// The game's screen, laid out as the game lays it out: primitives, textures, pixels per point.
type Drawn = (Vec<egui::ClippedPrimitive>, egui::TexturesDelta, f32);

impl Player {
    /// A line of what the script found, in the log.
    pub(crate) fn note(&mut self, text: &str) {
        println!("  {text}");
        let _ = writeln!(self.log, "  {text}");
    }

    /// The game's whole screen over the world seen from `camera` (`hud::game_screen`), laid out a
    /// few times so that windows opened are measured and shown, as a few frames of the game do.
    fn screen_drawn(&mut self, camera: &Camera, seen: &Inspection, tick: u64) -> Drawn {
        let (frames, ticks) = (TimingStats::new(1), TimingStats::new(1));
        let size = self.renderer.size();
        #[allow(clippy::cast_precision_loss)] // pixels
        let rect =
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.0 as f32, size.1 as f32));
        let mut textures = egui::TexturesDelta::default();
        let (mut shapes, mut ppp) = (Vec::new(), 1.0);
        // (Six passes a second apart, as `hud_shot.rs` found a scroll area needs to settle.)
        for _ in 0..6 {
            self.ui_time += 1.0;
            let mut input = egui::RawInput {
                screen_rect: Some(rect),
                time: Some(self.ui_time),
                ..egui::RawInput::default()
            };
            let viewport = input.viewports.entry(egui::ViewportId::ROOT).or_default();
            viewport.native_pixels_per_point = Some(1.0);
            let output = self.ui.run_ui(input, |ui| {
                let perf = PerfInfo {
                    frame_times: &frames,
                    tick_times: &ticks,
                    tick_rate_hz: crate::TICK_RATE_HZ,
                    tick,
                    adapter: "headless",
                };
                let _ = crate::hud::game_screen(
                    ui.ctx(),
                    &mut self.view,
                    camera,
                    (&perf, &mut self.profiler),
                    (&mut self.game, &mut self.screen),
                    (seen, tick),
                );
            });
            textures.append(output.textures_delta);
            (shapes, ppp) = (output.shapes, output.pixels_per_point);
        }
        (self.ui.tessellate(shapes, ppp), textures, ppp)
    }

    /// What the screen says now, in words for the log: the crosshair's name and prompts, the
    /// hands' prompts, the feedback; and what was heard since the last step.
    fn screen_said(&mut self, camera: &Camera) -> String {
        let target = crate::planet_aim::target(&self.view, camera);
        let (seen, name) = crate::hud_seen::seen(&self.view, target.as_ref());
        let p = crate::prompts::prompts(&seen);
        let keys = &self.game.bindings;
        let said = |list: &[crate::prompts::Prompt]| {
            (list.iter())
                .map(|x| format!("[{}] {}", keys.name(x.action), x.verb))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let kinds = self.view.sound.cost.kinds;
        let heard: Vec<u64> = (kinds.iter().zip(self.heard)).map(|(a, b)| a - b).collect();
        self.heard = kinds;
        let slowest = std::mem::take(&mut self.slowest) * 1e3;
        let tick = (self.view.session.tick_times()).and_then(|t| t.0.max_ms());
        format!(
            "\n    screen: {} {:?} | {} | hands: {} | heard (wind, bodies, breaks, fastenings, \
             blows, steps): {heard:?} | slowest frame {slowest:.0} ms, slowest tick {:.0} ms",
            name.unwrap_or_default(),
            p.state,
            said(&p.aimed),
            said(&p.hands),
            tick.unwrap_or(f64::NAN),
        )
    }
}

/// A white cross with a dark rim at the image's middle, as the game draws its crosshair.
fn crosshair(image: &mut creeer_render::Image) {
    let (cx, cy) = (image.width / 2, image.height / 2);
    let mut put = |x: u32, y: u32, v: u8| {
        let i = ((y * image.width + x) * 4) as usize;
        if let Some(p) = image.rgba.get_mut(i..i + 3) {
            p.copy_from_slice(&[v, v, v]);
        }
    };
    for d in 0..9u32 {
        for (w, v) in [(1u32, 20u8), (0, 240)] {
            for s in 0..=2 * w {
                put(cx - 4 + d, cy - w + s, v);
                put(cx - w + s, cy - 4 + d, v);
            }
        }
    }
}
