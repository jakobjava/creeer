//! The planet sandbox in the client: the planet's terrain streamed around the camera, the
//! world on it as the server tells it (one this process hosts, or a dedicated one:
//! `planet_net.rs`), the player's avatar to walk about with, day and night, digging and
//! filling.
//!
//! Everything is in the planet's rotating frame (centre at the origin, axis +Y). The sun turns
//! about the axis once per sidereal day (a stand-in until the orbits of F4).

use crate::physics_sandbox::PhysicsSnapshot;
use crate::planet_net::PlanetSession;
use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::message::Command;
use creeer_render::{
    Atmosphere, Camera, Renderer, SeaSurface, Sky, TerrainMeshData, TerrainVertex,
};
use creeer_terrain::Terrain;
use creeer_terrain::chunk::ChunkKey;
use creeer_terrain::stream::{StreamSettings, Streamer};
use std::sync::Arc;

/// Lowest height of the camera above the ground (m).
const CAMERA_CLEARANCE: f64 = 1.5;
/// Farthest a tool reaches (m).
pub const TOOL_REACH: f64 = 400.0;

/// What a click does at the crosshair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// Remove a ball of rock (2 m radius).
    Dig,
    /// Add a ball of rock.
    Fill,
    /// Bore a tunnel 12 m into the terrain along the view.
    Tunnel,
    /// Drop a crate.
    Crate,
    /// Drop a ball.
    Ball,
    /// Put down a steel frame.
    Frame,
    /// Build a powerhouse (its tank, turbine, generator and cells: F6.8's machine).
    Powerhouse,
}

impl Tool {
    /// All tools with their names.
    pub const ALL: [(Self, &'static str); 7] = [
        (Self::Dig, "dig"),
        (Self::Fill, "fill"),
        (Self::Tunnel, "tunnel"),
        (Self::Crate, "crate"),
        (Self::Ball, "ball"),
        (Self::Frame, "frame"),
        (Self::Powerhouse, "powerhouse"),
    ];
}

/// The running planet sandbox.
pub struct PlanetView {
    /// The planet's terrain as this client has it (shared with the terrain workers).
    pub terrain: Arc<Terrain>,
    /// The game.
    pub session: PlanetSession,
    /// Whether the player walks their avatar (else the camera flies free).
    pub walking: bool,
    /// The tool the player asked to hold (G1.4c), by its index in the tools' table.
    pub wielding: Option<u8>,
    streamer: Streamer<TerrainMeshData>,
    /// Edit count and generation the streamed chunks reflect.
    seen_edits: (u64, usize),
    /// Terrain chunks to draw this frame, with their cap faces.
    pub draw: Vec<(u64, u8)>,
    /// Active tool.
    pub tool: Tool,
    /// Rock used by the fill tool.
    pub fill_rock: u16,
    /// Time of day as a fraction of the sidereal day (0 = sun over +X).
    pub day_fraction: f64,
    /// How much faster than real time the sun moves.
    pub day_speed: f64,
}

impl std::fmt::Debug for PlanetView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanetView")
            .field("planet", &self.terrain.planet().name)
            .field("streamer", &self.streamer)
            .finish_non_exhaustive()
    }
}

/// A chunk mesh for the GPU: rock colours, openness, roughness.
pub fn render_mesh(terrain: &Terrain, key: ChunkKey) -> Option<TerrainMeshData> {
    let mesh = terrain.mesh_chunk(key)?;
    let rocks = &terrain.planet().geology.rocks.rocks;
    let vertices = mesh
        .positions
        .iter()
        .zip(&mesh.normals)
        .zip(&mesh.materials)
        .zip(&mesh.openness)
        .zip(mesh.coarse_normals.iter().zip(&mesh.morph))
        .map(|((((p, n), &m), &open), (coarse, &morph))| {
            let color = rocks.get(usize::from(m)).map_or([128; 3], |r| r.color);
            TerrainVertex::new(*p, *n, open, color, 0.88).with_coarser(*coarse, morph)
        })
        .collect();
    let index = |i: usize| u32::try_from(i).expect("chunk mesh too large");
    Some(TerrainMeshData {
        vertices,
        surface_indices: index(mesh.surface_indices),
        caps: mesh
            .cap_faces
            .clone()
            .map(|r| (index(r.start), index(r.end))),
        indices: mesh.indices,
        #[allow(clippy::cast_possible_truncation)] // chunk sizes fit f32
        chunk_size: key.size() as f32,
    })
}

impl PlanetView {
    /// A singleplayer game on the reference planet (its server hosted in this process).
    ///
    /// # Errors
    /// If the planet cannot be loaded.
    pub fn singleplayer() -> Result<Self, String> {
        let (session, terrain) = PlanetSession::singleplayer()?;
        Ok(Self::new(session, terrain))
    }

    /// A game on the dedicated server at `server`, as `name`.
    ///
    /// # Errors
    /// If it cannot be joined.
    pub fn connect(server: std::net::SocketAddr, name: &str) -> Result<Self, String> {
        let (session, terrain) = PlanetSession::connect(server, name)?;
        Ok(Self::new(session, terrain))
    }

    fn new(session: PlanetSession, terrain: Arc<Terrain>) -> Self {
        let streamer = Streamer::new(Arc::clone(&terrain), StreamSettings::default(), render_mesh);
        let fill_rock = terrain.planet().geology.rocks.rock("granite").unwrap_or(0);
        Self {
            session,
            walking: false,
            wielding: None,
            streamer,
            seen_edits: (0, 0),
            draw: Vec::new(),
            tool: Tool::Dig,
            fill_rock,
            day_fraction: 0.05,
            day_speed: 1.0,
            terrain,
        }
    }

    /// Asks the world for something.
    pub fn send(&mut self, command: Command) {
        self.session.command(command);
    }

    /// What there is to draw now.
    pub fn snapshot(&self) -> PhysicsSnapshot {
        self.session.snapshot()
    }

    /// Chunks in memory, and still to come (queued or being built).
    pub fn streaming(&self) -> (usize, usize) {
        (self.streamer.cached(), self.streamer.outstanding())
    }

    /// Streams terrain for `camera`: uploads finished chunks, frees evicted ones, rebuilds
    /// chunks the simulation's edits touched, and chooses the chunks to draw.
    pub fn update_terrain(&mut self, renderer: &mut Renderer, camera: DVec3) {
        let generation = self.session.connection.edits_generation;
        if generation != self.seen_edits.0 {
            // All edits were replaced: rebuild everything that is cached.
            self.streamer
                .invalidate(DVec3::splat(-1e9), DVec3::splat(1e9));
            self.seen_edits = (generation, 0);
        }
        let log_len = self.terrain.edits().len();
        if log_len > self.seen_edits.1 {
            let new: Vec<_> = self.terrain.edits().log()[self.seen_edits.1..].to_vec();
            for edit in new {
                let (lo, hi) = edit.brush().bounds();
                self.streamer.invalidate(lo, hi);
            }
            self.seen_edits.1 = log_len;
        }
        let update = self.streamer.update(camera);
        for (key, mesh) in &update.ready {
            renderer.insert_terrain(key.id(), key.origin(), mesh);
        }
        for key in &update.evicted {
            renderer.remove_terrain(key.id());
        }
        self.draw.clear();
        self.draw
            .extend(update.draw.iter().map(|(key, caps)| (key.id(), *caps)));
    }

    /// Height of `p` above the ground below it (m; negative underground).
    pub fn altitude(&self, p: DVec3) -> f64 {
        self.terrain.density(p)
    }

    /// The walk asked of the avatar: `forward` and `right` (−1 to 1 each) along the camera's
    /// heading on the avatar's horizon, at `speed` (m/s).
    pub fn walk(&self, camera: &Camera, (forward, right): (f64, f64), speed: f64) -> DVec3 {
        let Some(avatar) = self.session.connection.avatar() else {
            return DVec3::ZERO;
        };
        let up = avatar.position.normalize();
        let flat = |v: DVec3| (v - up * v.dot(up)).normalize_or_zero();
        (flat(camera.forward()) * forward + flat(camera.right()) * right).normalize_or_zero()
            * speed
    }

    /// The jetpack's thrust asked for (a fraction of its most): up, leaning the way `walk` goes,
    /// while `on`.
    pub fn thrust(&self, walk: DVec3, on: bool) -> DVec3 {
        let Some(avatar) = self.session.connection.avatar().filter(|_| on) else {
            return DVec3::ZERO;
        };
        (avatar.position.normalize() + walk.normalize_or_zero() * 0.4).clamp_length_max(1.0)
    }

    /// Puts the camera behind and above the avatar, looking the way it looks, never in the
    /// ground.
    pub fn follow(&self, camera: &mut Camera) {
        let Some(avatar) = self.session.connection.avatar() else {
            return;
        };
        let up = avatar.position.normalize();
        camera.level_to(up);
        camera.position = avatar.position + up * 1.2 - camera.forward() * 4.5;
        self.constrain_camera(camera);
    }
    /// Keeps the camera level with the local horizon and above the ground.
    pub fn constrain_camera(&self, camera: &mut Camera) {
        let up = camera.position.normalize();
        camera.level_to(up);
        let height = self.altitude(camera.position);
        if height < CAMERA_CLEARANCE {
            camera.position += up * (CAMERA_CLEARANCE - height);
        }
    }

    /// Unit vector towards the sun in the planet frame.
    pub fn sun_direction(&self) -> DVec3 {
        self.sun_direction_at(self.day_fraction)
    }

    /// Unit vector towards the sun at time of day `fraction`: it turns about the axis
    /// opposite to the planet's spin.
    pub fn sun_direction_at(&self, fraction: f64) -> DVec3 {
        DQuat::from_rotation_y(-std::f64::consts::TAU * fraction) * DVec3::X
    }

    /// Advances the time of day by a frame of `dt` seconds.
    pub fn advance_day(&mut self, dt: f64) {
        let day = self.terrain.planet().sidereal_day;
        self.day_fraction = (self.day_fraction + dt * self.day_speed / day).rem_euclid(1.0);
    }

    /// The planet's sky: an Earth-like atmosphere scaled to its radius.
    pub fn sky(&self) -> Sky {
        let planet = self.terrain.planet();
        // Its ocean at sea level, the datum (the planet turns about +Y in its own frame).
        let sea = planet.ocean.map(|_| SeaSurface {
            polar_radius: planet.datum.polar_radius,
            equatorial_radius: planet.datum.equatorial_radius(),
            axis: DVec3::Y,
            color: SeaSurface::OCEAN,
        });
        Sky {
            atmosphere: Atmosphere::earthlike(planet.radius),
            planet_center: DVec3::ZERO,
            sun_irradiance: [3.6, 3.4, 3.1],
            star_brightness: 1.0,
            sea,
        }
    }

    /// Where the view ray from `origin` along `dir` meets the ground, if within `reach`.
    pub fn raycast(&self, origin: DVec3, dir: DVec3, reach: f64) -> Option<DVec3> {
        let mut t = 0.0;
        while t < reach {
            let p = origin + dir * t;
            let d = self.terrain.density(p);
            if d < 0.02 {
                return Some(p);
            }
            // The density is the height above the ground: on slopes it overestimates the
            // distance, so step only part of it.
            t += (0.35 * d).max(0.05);
        }
        None
    }

    /// Applies the active tool where the camera looks; walking with a tool in hand, strikes what
    /// the crosshair is on if it is within reach (G1.4e).
    pub fn use_tool(&mut self, camera: &Camera) {
        if self.walking && self.carrying() {
            // Thrown the way the camera looks (G1.4f).
            self.send(Command::Throw(camera.forward()));
            return;
        }
        if self.walking && self.wielding.is_some() {
            if let Some(target) = crate::planet_aim::target(self, camera).filter(|t| t.in_reach) {
                self.session.connection.strike(target.aimed);
            }
            return;
        }
        let dir = camera.forward();
        let Some(hit) = self.raycast(camera.position, dir, TOOL_REACH) else {
            return;
        };
        let up = hit.normalize();
        use creeer_terrain::edits::{Brush, Edit};
        let edit = match self.tool {
            Tool::Dig => Edit::Dig(Brush::Sphere {
                center: hit,
                radius: 2.0,
            }),
            Tool::Fill => Edit::Fill(
                Brush::Sphere {
                    center: hit + up * 1.0,
                    radius: 2.0,
                },
                self.fill_rock,
            ),
            Tool::Tunnel => Edit::Dig(Brush::Capsule {
                a: hit - dir * 0.5,
                b: hit + dir * 12.0,
                radius: 2.5,
            }),
            Tool::Crate => return self.send(Command::Crate(hit + up * 2.0)),
            Tool::Ball => return self.send(Command::Ball(hit + up * 2.0)),
            // The server lifts it clear of the ground (on a slope, of its corners too).
            Tool::Frame => return self.send(Command::Frame(hit)),
            Tool::Powerhouse => return self.send(Command::Powerhouse(hit)),
        };
        self.session.edit(edit);
    }

    /// A camera far out in space, looking at the sunlit side of the planet.
    pub fn orbit_camera(&self) -> Camera {
        let sun = self.sun_direction();
        let toward = (sun + DVec3::Y * 0.35).normalize();
        place(toward * (self.terrain.planet().radius * 3.2), -toward)
    }

    /// A camera `height` metres above the ground in direction `up`, looking along `heading`
    /// (tangent) and `pitch` radians down.
    pub fn ground_camera(&self, up: DVec3, height: f64, heading: DVec3, pitch: f64) -> Camera {
        let up = up.normalize();
        let ground =
            up * (self.terrain.planet().datum.radius(up) + self.terrain.surface_height(up));
        let (s, c) = det::sin_cos(pitch);
        place(ground + up * height, heading * c - up * s)
    }
}

/// A camera at `position` looking along `forward`, level with the local horizon.
pub fn place(position: DVec3, forward: DVec3) -> Camera {
    place_up(position, forward, position.normalize())
}

/// A camera at `position` looking along `forward`, level with the horizon of `up` (for
/// frames not centred on a planet).
pub fn place_up(position: DVec3, forward: DVec3, up: DVec3) -> Camera {
    let mut camera = Camera::new(position);
    camera.level_to(up);
    // Yaw and pitch of `forward` in the horizon frame.
    let local = camera.frame.inverse() * forward.normalize();
    camera.yaw = det::atan2(-local.x, -local.z);
    camera.pitch = det::asin(local.y.clamp(-0.995, 0.995));
    camera
}
