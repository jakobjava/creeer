//! Building played as the player plays it, a frame at every step (G2.11 B, `docs/playability-
//! plan.md` rule 2): `creeer_client --build-play <bridge|frame|site> <dir>`.
//!
//! A singleplayer game (its server hosted here, as the game hosts it) is played in real time
//! through the client's own path: the camera at the eyes looking where the script looks, the
//! held part placed each frame as `holding.rs` places it, the keys pressed as a player presses
//! them, through the controls' table and the game's own actions (`bindings.rs`,
//! `PlanetView::act`: G takes and puts down, R stands up or lays down, Q with the mouse turns, J
//! fastens, P sets a post). Each step saves `<dir>/<nn>-<step>.png` (first person) and a line of
//! what the player was told and where the held part is (`<dir>/steps.txt`).
//!
//! `site` (G2.11 Q, the play-through) is the user's first ten minutes at the bridge site
//! (`bridge_site.rs`, `build_play_site.rs`): its frames have the game's whole screen drawn over
//! them (`hud::game_screen`: the HUD, the kit, the inventory, the menu, the overlay's legend and
//! what failed), as the player sees it, and its log what was heard at each step.

use crate::DemoMeshes;
use crate::bindings::{Action, Context, Input, entry};
use crate::menu::Screen;
use crate::people::People;
use crate::planet_view::PlanetView;
use crate::settings::GameSettings;
use creeer_math::glam::DVec3;
use creeer_render::{Camera, Renderer};
use creeer_server::world::site::BridgeSite;
use creeer_terrain::edits::{Brush, Edit};
use creeer_ui::egui;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

/// The scripts this module plays.
pub(crate) const SCRIPTS: [&str; 5] = ["bridge", "frame", "site", "site-walk", "site-load"];
/// Walking speed (m/s), as the game walks.
const WALK: f64 = 3.0;
/// A client frame (s).
pub(crate) const STEP: f64 = 0.01;
/// What a script is stopped with after `--until`'s steps.
pub(crate) const UNTIL: &str = "played as far as asked";

/// The game being played, and what draws it (its frames: `build_play_shot.rs`).
pub(crate) struct Player {
    pub(crate) view: PlanetView,
    pub(crate) renderer: Renderer,
    pub(crate) meshes: DemoMeshes,
    pub(crate) people: People,
    pub(crate) dir: PathBuf,
    pub(crate) shots: u32,
    pub(crate) log: String,
    /// Where the eyes look (a point), and the way east and north at the start.
    pub(crate) look: DVec3,
    pub(crate) start: DVec3,
    pub(crate) east: DVec3,
    pub(crate) north: DVec3,
    /// The camera last looked from.
    pub(crate) camera: Camera,
    pub(crate) last_shot: Instant,
    last_frame: Instant,
    /// How many steps to play (`--until`).
    pub(crate) until: u32,
    /// The bridge site, when played there: then the frames show the game's whole screen
    /// (`hud`), drawn by `ui` with the player's `screen` and `game` settings, and what is heard
    /// is gathered each frame (`view.sound`, recorded, not played).
    pub(crate) site: Option<BridgeSite>,
    pub(crate) hud: bool,
    pub(crate) ui: egui::Context,
    pub(crate) ui_time: f64,
    pub(crate) screen: Screen,
    pub(crate) game: GameSettings,
    pub(crate) profiler: crate::profiler_ui::ProfilerWindow,
    /// The sounds asked for by kind when the last step was logged (`Sounds::cost`), and the
    /// slowest of the script's frames since (s), the one after a frame was saved left out.
    pub(crate) heard: [u64; 6],
    pub(crate) slowest: f64,
    pub(crate) shot_taken: bool,
}

/// Plays `--build-play <script> <dir>`.
///
/// # Errors
/// If the game cannot be started, a step cannot be played or a frame not saved.
pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let at = (args.iter().position(|a| a == "--build-play")).ok_or("--build-play")?;
    let script = args.get(at + 1).ok_or("--build-play needs a script")?;
    let dir = PathBuf::from(args.get(at + 2).ok_or("--build-play needs a directory")?);
    std::fs::create_dir_all(&dir)?;
    let site = match script.as_str() {
        "site" => Some(false),
        "site-walk" | "site-load" => Some(true),
        _ => None,
    };
    let mut player = Player::start(dir, site)?;
    // `--until n`: the first n steps only.
    player.until = (args.iter().position(|a| a == "--until"))
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(u32::MAX);
    let played = match script.as_str() {
        "bridge" => crate::build_play_scripts::bridge(&mut player),
        "frame" => crate::build_play_scripts::frame(&mut player),
        "site" => crate::build_play_site::ten_minutes(&mut player),
        "site-walk" => crate::build_play_site::on_the_built_bridge(&mut player, true),
        "site-load" => crate::build_play_site::on_the_built_bridge(&mut player, false),
        other => return Err(format!("no script {other}: {SCRIPTS:?}").into()),
    };
    match played {
        Err(e) if e == UNTIL => {}
        other => other?,
    }
    std::fs::write(player.dir.join("steps.txt"), &player.log)?;
    Ok(())
}

impl Player {
    /// A singleplayer creative game, or the bridge site as `--bridge` starts it (`site`: with
    /// *bridge_demo*'s footbridge built across, if true).
    fn start(dir: PathBuf, site: Option<bool>) -> Result<Self, Box<dyn std::error::Error>> {
        let at_site = site.is_some();
        let (mut view, site) = match site {
            Some(built) => {
                let (view, site) = if built {
                    crate::bridge_site::view_built()?
                } else {
                    crate::bridge_site::view()?
                };
                (view, Some(site))
            }
            None => (PlanetView::singleplayer(true)?, None),
        };
        crate::planet_shot_build::play(&mut view, 2.0)?;
        view.walking = true;
        let me = (view.session.connection.avatar())
            .ok_or("no avatar")?
            .position;
        let up = me.normalize();
        if site.is_none() {
            view.day_fraction = crate::planet_shot::day_for_sun(&view, up, 40.0);
        } else {
            // What is heard, gathered as the game gathers it (recorded: no device opened).
            view.sound = crate::sounds::Sounds::recording();
        }
        view.day_speed = 0.0;
        let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
        // The game's window opens at 1600 × 900; the frames of the screen are of that size.
        let size = if at_site { (1600, 900) } else { (1280, 720) };
        let mut renderer = pollster::block_on(Renderer::new_headless(size))?;
        let meshes = DemoMeshes::register_for(&mut renderer);
        let people = People::register(&mut renderer)?;
        let look = me + east * 5.0;
        let camera = crate::planet_view::place(me, east);
        let ui = egui::Context::default();
        creeer_ui::theme::apply(&ui);
        Ok(Self {
            hud: at_site,
            site,
            ui,
            ui_time: 0.0,
            screen: Screen::default(),
            game: GameSettings::default(),
            profiler: crate::profiler_ui::ProfilerWindow::default(),
            heard: [0; 6],
            slowest: 0.0,
            shot_taken: false,
            view,
            renderer,
            meshes,
            people,
            dir,
            shots: 0,
            log: String::new(),
            look,
            start: me,
            east,
            north,
            camera,
            last_shot: Instant::now(),
            last_frame: Instant::now(),
            until: u32::MAX,
        })
    }

    /// The avatar's centre as drawn.
    pub(crate) fn me(&self) -> DVec3 {
        self.view
            .session
            .connection
            .avatar()
            .map_or(DVec3::ZERO, |a| a.position)
    }

    pub(crate) fn up(&self) -> DVec3 {
        self.me().normalize()
    }

    /// The eyes.
    pub(crate) fn eye(&self) -> DVec3 {
        creeer_sim::reach::eye(self.me(), self.up())
    }

    /// The ground `e` m east and `n` m north of `from` (on the terrain as this client has it).
    pub(crate) fn ground_at(&self, from: DVec3, e: f64, n: f64) -> DVec3 {
        let p = from + self.east * e + self.north * n;
        let t = &self.view.terrain;
        p.normalize() * (t.planet().datum.radius(p) + t.surface_height(p))
    }

    /// The camera at the eyes looking at `self.look`, as first person puts it.
    fn camera(&self) -> Camera {
        let eye = self.eye();
        let way = (self.look - eye).try_normalize().unwrap_or(self.east);
        let mut c = crate::planet_view::place(eye, way);
        c.level_to(self.up());
        c
    }

    /// Plays `seconds` looking at `self.look`, walking `walk` (a level way, m/s).
    pub(crate) fn play(&mut self, seconds: f64, walk: DVec3) {
        let start = Instant::now();
        while start.elapsed().as_secs_f64() < seconds {
            self.frame(walk);
            std::thread::sleep(Duration::from_secs_f64(STEP));
        }
    }

    /// One client frame: the hold placed where the crosshair meets something, then the inputs,
    /// the client's time moved on as the server's (it runs in real time on its own thread: a
    /// client running slower than it, its last input repeated, walked on past where it stopped).
    pub(crate) fn frame(&mut self, walk: DVec3) {
        let dt = self.last_frame.elapsed().as_secs_f64().min(0.1);
        self.last_frame = Instant::now();
        let camera = self.camera();
        let v = &mut self.view;
        let surface = v.holding.holding().then(|| {
            let c = &v.session.connection;
            let trees = v.trees.as_ref();
            c.surface_aimed(camera.rotation(), |from, along, most| {
                trees.and_then(|t| t.aim(c, from, along, most)).map(|a| a.2)
            })
        });
        v.holding
            .update(&camera, &mut v.session.connection, surface.flatten());
        v.session.update(dt, (walk, false, DVec3::ZERO));
        self.camera = camera;
        if self.hud {
            self.hear(dt);
        }
    }

    /// What is heard this frame, as the game gathers it (`app/planet.rs`): the people animated
    /// (their feet step), then the sounds of what happened.
    fn hear(&mut self, dt: f64) {
        // (The frame after a frame was saved waited for the saving: not a frame of the game's.)
        if !std::mem::take(&mut self.shot_taken) {
            self.slowest = self.slowest.max(dt);
        }
        // (The people only: the trees and the loose things drawn too cost a frame milliseconds.)
        let snapshot = self.view.session.snapshot();
        let ground = crate::people_ground::PlanetGround {
            terrain: &self.view.terrain,
        };
        let own = (self.view.session.connection.welcome.as_ref()).map(|w| w.avatar.0);
        self.people.first_person = own.filter(|_| self.view.first_person);
        let mut scratch = creeer_render::Scene::default();
        self.people
            .draw(&snapshot.people, dt, &ground, &mut scratch);
        let mut sound = std::mem::take(&mut self.view.sound);
        sound.frame(&self.view, &self.camera, Some(&self.people), dt);
        self.view.sound = sound;
    }

    /// Walks towards `to` (level) until within `within` m, looking where she goes (`look_down`
    /// m below the eyes' level, 3 m ahead).
    pub(crate) fn walk_to(&mut self, to: DVec3, within: f64, look_down: f64) {
        // (Not getting there in 20 s, she gives up: stuck in the gully, against its wall.)
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(20) {
            let up = self.up();
            let way = (to - self.me()).reject_from(up);
            if way.length() < within {
                break;
            }
            // Slowing the last metre and a half before where she stops, as anyone walking up to
            // an edge does (G2.11 Q: slowed towards `to` itself, she stopped 1.3 m short of it at
            // 2.6 m/s, slid on 1.2 m and stood on the stringer she meant to take).
            let speed = ((way.length() - within) * 2.0).clamp(0.5, WALK);
            let way = way.normalize();
            self.look = self.eye() + way * 3.0 - up * look_down;
            self.play(STEP, way * speed);
        }
        self.play(0.3, DVec3::ZERO);
    }

    /// A key pressed and let go, as the game takes it (`app/input.rs`): the actions the
    /// controls' table binds to it, in the game's order, the first that applies done (the held
    /// part's keys before the world's; with the menu open, only the menu's).
    pub(crate) fn key(&mut self, code: KeyCode) {
        self.press(Input::Key(code));
    }

    /// A key or button pressed and let go (`key`); moving keys are the script's own walking.
    pub(crate) fn press(&mut self, input: Input) {
        let camera = self.camera();
        let menu = self.screen.menu.is_some();
        for action in self.game.bindings.in_order(input) {
            let e = entry(action);
            if menu && e.context != Context::Always {
                continue;
            }
            let done = match action {
                _ if e.context == Context::Holding => {
                    let h = &mut self.view.holding;
                    let took = h.key(action, true);
                    h.key(action, false);
                    took
                }
                Action::Menu => {
                    self.screen.escape(Some(&mut self.view.inventory));
                    true
                }
                Action::Debug => {
                    self.screen.debug = !self.screen.debug;
                    true
                }
                Action::Kit => {
                    self.screen.kit = !self.screen.kit;
                    true
                }
                Action::Use => {
                    self.view.use_tool(&camera);
                    true
                }
                _ if e.held => true,
                _ => self.view.act(action, &camera),
            };
            if done {
                return;
            }
        }
    }

    /// Q held, the mouse moved `(dx, dy)` pixels, Q let go.
    pub(crate) fn turn(&mut self, dx: f64, dy: f64) {
        let h = &mut self.view.holding;
        h.key(Action::Turn, true);
        h.mouse((dx, dy));
        h.key(Action::Turn, false);
    }

    /// Q with the mouse until what is held lies `yaw` from across the view (rad: a quarter turn
    /// lays it along the view), as a player turns it watching it.
    pub(crate) fn turn_to(&mut self, yaw: f64) {
        let view = self.camera().rotation();
        let now = (self.view.session.connection.held_turn(view)).map_or(0.0, |t| t.0);
        // (The mouse turns it 0.006 rad a pixel: `holding.rs`.)
        self.turn((yaw - now) / 0.006, 0.0);
    }

    /// Digs `brush` as the sandbox's tools do, and waits for it.
    pub(crate) fn dig(&mut self, brush: Brush) {
        self.view.session.edit(Edit::Dig(brush));
        self.play(0.3, DVec3::ZERO);
    }

    /// Drops the kit's `name` at `at`.
    pub(crate) fn kit(&mut self, name: &str, at: DVec3) -> Result<(), String> {
        let data = creeer_data::Data::base();
        let kit = creeer_sim::items::kit::kit(&data)?;
        let k = (kit.iter().position(|k| k.name == name)).ok_or(format!("no {name}"))?;
        let item = u8::try_from(k).map_err(|e| e.to_string())?;
        self.view
            .send(creeer_net::message::Command::Kit { item, at });
        Ok(())
    }

    /// The bodies drawn nearest `at` first (the avatar left out): id and pose.
    pub(crate) fn nearest(&self, at: DVec3) -> Vec<(creeer_net::entity::EntityId, DVec3)> {
        let c = &self.view.session.connection;
        let own = c.welcome.as_ref().map(|w| w.avatar);
        let mut all: Vec<_> = (c.poses().into_iter())
            .filter(|(id, _)| Some(*id) != own)
            .map(|(id, p)| (id, p.position))
            .collect();
        all.sort_by(|a, b| (a.1 - at).length().total_cmp(&(b.1 - at).length()));
        all
    }
}
