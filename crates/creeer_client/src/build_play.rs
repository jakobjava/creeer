//! Building played as the player plays it, a frame at every step (G2.11 B, `docs/playability-
//! plan.md` rule 2): `creeer_client --build-play <bridge|frame> <dir>`.
//!
//! A singleplayer game (its server hosted here, as the game hosts it) is played in real time
//! through the client's own path: the camera at the eyes looking where the script looks, the
//! held part placed each frame as `holding.rs` places it, the keys pressed as a player presses
//! them (G takes and puts down, R stands up or lays down, Q with the mouse turns, J fastens, P
//! sets a post). Each step saves `<dir>/<nn>-<step>.png` (first person, the crosshair drawn) and
//! a line of what the player was told and where the held part is (`<dir>/steps.txt`).

use crate::DemoMeshes;
use crate::bindings::Action;
use crate::people::People;
use crate::planet_view::PlanetView;
use creeer_math::glam::DVec3;
use creeer_net::message::Command;
use creeer_render::{Camera, Renderer};
use creeer_terrain::edits::{Brush, Edit};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

/// The scripts this module plays (`deck`: at the bridge site, `build_play_deck.rs`).
pub(crate) const SCRIPTS: [&str; 3] = ["bridge", "frame", "deck"];
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
    // The deck is built at the bridge site (ADR-0039); the others where a game starts.
    let site = if script == "deck" {
        let (view, site) = crate::bridge_site::view()?;
        Some((view, site))
    } else {
        None
    };
    let (view, site) = match site {
        Some((view, site)) => (view, Some(site)),
        None => (PlanetView::singleplayer(true)?, None),
    };
    let mut player = Player::start(dir, view)?;
    // `--until n`: the first n steps only.
    player.until = (args.iter().position(|a| a == "--until"))
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(u32::MAX);
    let played = match (script.as_str(), &site) {
        ("bridge", _) => crate::build_play_scripts::bridge(&mut player),
        ("frame", _) => crate::build_play_scripts::frame(&mut player),
        ("deck", Some(site)) => crate::build_play_deck::deck(&mut player, site),
        (other, _) => return Err(format!("no script {other}: {SCRIPTS:?}").into()),
    };
    match played {
        Err(e) if e == UNTIL => {}
        other => other?,
    }
    std::fs::write(player.dir.join("steps.txt"), &player.log)?;
    Ok(())
}

impl Player {
    fn start(dir: PathBuf, mut view: PlanetView) -> Result<Self, Box<dyn std::error::Error>> {
        crate::planet_shot_build::play(&mut view, 2.0)?;
        view.walking = true;
        let me = (view.session.connection.avatar())
            .ok_or("no avatar")?
            .position;
        let up = me.normalize();
        view.day_fraction = crate::planet_shot::day_for_sun(&view, up, 40.0);
        view.day_speed = 0.0;
        let (east, north) = creeer_terrain::geology::deposits::tangent_frame(up);
        let mut renderer = pollster::block_on(Renderer::new_headless((1280, 720)))?;
        let meshes = DemoMeshes::register_for(&mut renderer);
        let people = People::register(&mut renderer)?;
        let look = me + east * 5.0;
        let camera = crate::planet_view::place(me, east);
        Ok(Self {
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
    }

    /// Walks towards `to` (level) until within `within` m, looking where she goes (`look_down`
    /// m below the eyes' level, 3 m ahead).
    pub(crate) fn walk_to(&mut self, to: DVec3, within: f64, look_down: f64) {
        for _ in 0..2000 {
            let up = self.up();
            let way = (to - self.me()).reject_from(up);
            if way.length() < within {
                break;
            }
            // Slowing the last metre and a half, as anyone walking up to an edge does.
            let speed = (way.length() * 2.0).clamp(0.5, WALK);
            let way = way.normalize();
            self.look = self.eye() + way * 3.0 - up * look_down;
            self.play(STEP, way * speed);
        }
        self.play(0.3, DVec3::ZERO);
    }

    /// A key pressed and let go, as the game takes it: the held part's keys first, then the
    /// world's (G takes or puts down, J fastens, P sets a post, K the next fastener).
    pub(crate) fn key(&mut self, code: KeyCode) {
        let camera = self.camera();
        let v = &mut self.view;
        // The held part's keys, as the controls' table names them (`bindings.rs`).
        let held = match code {
            KeyCode::KeyR => Some(Action::StandOrLay),
            KeyCode::KeyQ => Some(Action::Turn),
            KeyCode::ControlLeft | KeyCode::ControlRight => Some(Action::Fine),
            _ => None,
        };
        if let Some(action) = held
            && v.holding.key(action, true)
        {
            v.holding.key(action, false);
            return;
        }
        match code {
            KeyCode::KeyG => v.grab_or_drop(&camera),
            KeyCode::KeyJ => v.fasten(&camera),
            KeyCode::KeyP => v.set_post(&camera),
            KeyCode::KeyK => v.fastening.next(),
            KeyCode::KeyT => v.send(Command::Throw(camera.forward())),
            _ => {}
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
        self.view.send(Command::Kit { item, at });
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
