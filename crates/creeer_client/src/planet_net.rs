//! The planet sandbox as a client of a server (F5, ADR-0008): of one it hosts itself for a
//! singleplayer game (on a thread of its own, joined by an in-process connection), or of a
//! dedicated server over UDP (`--connect`). Either way the window only draws what it is
//! told and asks for what the player does; the world is the server's.

use crate::animate::Body;
use crate::people::DrawnPerson;
use crate::physics_sandbox::{Look, PhysicsSnapshot};
use creeer_math::Frame64;
use creeer_math::glam::DQuat;
use creeer_math::glam::DVec3;
use creeer_net::channel::TICK_RATE;
use creeer_net::entity::{EntityId, Kind, Shape};
use creeer_net::message::Command;
use creeer_server::client::Status;
use creeer_server::host::{Host, read_save, write_save};
use creeer_server::world::load_terrain;
use creeer_server::{Connection, Server, ServerWorld};
use creeer_sim::character::bones;
use creeer_terrain::Terrain;
use creeer_terrain::edits::{Edit, Edits};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The planet a singleplayer game is on.
pub const PLANET: &str = "planets/home.ron";
/// The player's name in a singleplayer game.
const PLAYER: &str = "player";

/// A game: the connection, and the server if this process hosts it.
pub struct PlanetSession {
    /// The connection to the server.
    pub connection: Connection,
    host: Option<Host>,
    /// Client time not yet ticked (s).
    behind: f64,
}

impl std::fmt::Debug for PlanetSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanetSession")
            .field("connection", &self.connection)
            .field("hosted", &self.host.is_some())
            .finish_non_exhaustive()
    }
}

impl PlanetSession {
    /// A singleplayer game on the reference planet (`creative`: the sandbox's tools, the
    /// jetpack and the kit; else survival, G1.6b): a server hosted in this process, and
    /// this client joined to it. Returns it with the client's terrain.
    ///
    /// # Errors
    /// If the planet cannot be loaded.
    pub fn singleplayer(creative: bool) -> Result<(Self, Arc<Terrain>), String> {
        let data = crate::content::data();
        let mut world = ServerWorld::new(Arc::clone(&data), load_terrain(&data, PLANET)?, PLANET);
        // Survival (G1.6b); creative, the sandbox's tools drop crates and dig within 500 m.
        world.creative = creative;
        let mut server = Server::new(world);
        let end = server.connect_memory();
        let mut connection = Connection::memory(end, PLAYER, data.hash());
        let terrain = load_terrain(&data, PLANET)?;
        connection.attach_terrain(Arc::clone(&terrain), &data);
        let session = Self {
            connection,
            host: Some(Host::spawn(server)),
            behind: 0.0,
        };
        Ok((session, terrain))
    }

    /// A game on the dedicated server at `server`, as `name`: waits (up to ten seconds) to
    /// be welcomed, then loads the planet it names. Returns it with the client's terrain.
    ///
    /// # Errors
    /// If the server cannot be reached, refuses, or names a planet this client lacks.
    pub fn connect(server: SocketAddr, name: &str) -> Result<(Self, Arc<Terrain>), String> {
        let data = crate::content::data();
        let mut connection = Connection::udp(server, name, data.hash())?;
        let start = Instant::now();
        let step = Duration::from_millis(10);
        loop {
            connection.update(step);
            connection.flush();
            match connection.status() {
                Status::Joined => break,
                Status::Refused(why) => return Err(format!("refused: {why}")),
                Status::Closed(why) => return Err(format!("could not connect: {why}")),
                Status::Connecting if start.elapsed() > Duration::from_secs(10) => {
                    return Err(format!("no answer from {server}"));
                }
                Status::Connecting => std::thread::sleep(step),
            }
        }
        let planet = connection
            .welcome
            .as_ref()
            .map(|w| w.planet.clone())
            .unwrap_or_default();
        let terrain = load_terrain(&data, &planet)?;
        connection.attach_terrain(Arc::clone(&terrain), &data);
        let session = Self {
            connection,
            host: None,
            behind: 0.0,
        };
        Ok((session, terrain))
    }

    /// Whether this process hosts the game (it may save and load it).
    pub fn hosted(&self) -> bool {
        self.host.is_some()
    }

    /// A frame of `dt` seconds passed: takes in what the server sent, and runs the client's
    /// ticks due (each the player's input `walk`, `jump` and `thrust` for its avatar), each at
    /// its moment within the frame, so the avatar is drawn the time since its last tick on.
    pub fn update(&mut self, dt: f64, (walk, jump, thrust): (DVec3, bool, DVec3)) {
        let (to_each, after, behind) = frame_ticks(self.behind, dt);
        for to in to_each {
            self.connection.update(Duration::from_secs_f64(to));
            self.connection.input(walk, jump, thrust);
            self.connection.flush();
        }
        self.connection.update(Duration::from_secs_f64(after));
        self.behind = behind;
    }

    /// Asks the world for something.
    pub fn command(&mut self, command: Command) {
        self.connection.command(command);
    }

    /// Asks for a terrain edit.
    pub fn edit(&mut self, edit: Edit) {
        let mut one = Edits::default();
        one.push(edit);
        self.command(Command::Edit(one.to_bytes()));
    }

    /// Saves the world to `path` (hosted games only).
    ///
    /// # Errors
    /// If this process does not host the game, or the file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let host = self.host.as_ref().ok_or("only the host saves the world")?;
        write_save(path, &host.save()?).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Loads the world from `path`, a save of any version (hosted games only).
    ///
    /// # Errors
    /// If this process does not host the game, or the file is no save of this planet.
    pub fn load(&self, path: &Path) -> Result<(), String> {
        let host = self.host.as_ref().ok_or("only the host loads a world")?;
        host.load(read_save(path)?)
    }

    /// The server's tick times, if this process hosts it.
    pub fn tick_times(&self) -> Option<(creeer_core::TimingStats, u64)> {
        self.host.as_ref().map(|h| (h.tick_times(), h.tick()))
    }

    /// What there is to draw: every entity in view where it is drawn now.
    pub fn snapshot(&self) -> PhysicsSnapshot {
        let c = &self.connection;
        let mut out = PhysicsSnapshot::default();
        let poses = c.poses();
        // The made tools in hands, drawn in them, not as bodies (G1.10).
        let in_hand: std::collections::BTreeSet<EntityId> = (c.kinds.values())
            .filter_map(|k| match k {
                Kind::Character { carries, .. } => *carries,
                _ => None,
            })
            .filter(|t| c.items.get(t).is_some_and(|l| l.grip.is_some()))
            .collect();
        for (id, kind) in &c.kinds {
            let Some(pose) = poses.get(id) else { continue };
            let (p, q) = (pose.position, pose.orientation);
            match kind {
                Kind::Body(Shape::Box(half), matter) => {
                    out.boxes.push((p, q, *half));
                    out.box_look
                        .push(matter.colour.map_or(Look::Crate, Look::Colour));
                    // Its age as of the moment drawn (G1.3c).
                    let age = c.ages.get(id).map_or((0.0, 0.0), |a| {
                        let exposure = a.exposure_at(c.drawn_at(), TICK_RATE);
                        #[allow(clippy::cast_possible_truncation)] // years and a share
                        let drawn = (exposure as f32, a.wear as f32);
                        drawn
                    });
                    out.box_age.push(age);
                }
                Kind::Body(Shape::Sphere(radius), _) => out.spheres.push((p, *radius)),
                // A character (G1.3e): standing, its body's frame; fallen, its segments'.
                Kind::Character {
                    look,
                    fallen,
                    wields,
                    carries,
                    ..
                } => {
                    let body = if *fallen {
                        fallen_bones(*id, (p, q), c).map(Body::Fallen)
                    } else {
                        Some(Body::Standing(p, q))
                    };
                    // A made tool in one hand (G1.10), drawn there, not as its body.
                    let made = (carries.and_then(|t| c.items.get(&t)))
                        .and_then(|l| Some((l.parts.clone(), l.grip?)));
                    if let Some(body) = body {
                        let look = look.clone();
                        out.people.push(DrawnPerson {
                            key: id.0,
                            look,
                            body,
                            wields: *wields,
                            swing: c.swinging(*id),
                            carrying: carries.is_some() && made.is_none(),
                            made,
                        });
                    }
                }
                // Drawn with its person.
                Kind::Segment { .. } => {}
                // An item's hull (a stone, a stick: G1.6c) as the box about it in its colour; the
                // sandbox's other hulls as the ball around them.
                Kind::Body(Shape::Hull(corners), matter) => {
                    if in_hand.contains(id) {
                        continue;
                    }
                    let parts = c.items.get(id).map(|l| &l.parts).filter(|p| !p.is_empty());
                    if let Some(parts) = parts {
                        // A made thing as its pieces (G1.6).
                        for part in parts {
                            out.boxes.push((p + q * part.at, q * part.turn, part.half));
                            out.box_look.push(Look::Colour(part.colour));
                            out.box_age.push((0.0, 0.0));
                        }
                    } else if let Some(colour) = matter.colour {
                        let (lo, hi) = (corners.iter())
                            .fold((DVec3::MAX, DVec3::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
                        out.boxes
                            .push((p + q * ((lo + hi) * 0.5), q, (hi - lo) * 0.5));
                        out.box_look.push(Look::Colour(colour));
                        out.box_age.push((0.0, 0.0));
                    } else {
                        let radius = corners.iter().map(|v| v.length()).fold(0.0, f64::max);
                        out.spheres.push((p, radius));
                    }
                }
                // Drawn as their trees (`tree_view.rs`).
                Kind::Tree(_) => {}
                Kind::Structure(frame) => {
                    let broken = c.broken.get(id);
                    for (k, [a, b]) in frame.members.iter().enumerate() {
                        let (Some(na), Some(nb)) =
                            (frame.nodes.get(*a as usize), frame.nodes.get(*b as usize))
                        else {
                            continue;
                        };
                        #[allow(clippy::cast_possible_truncation)] // fewer than 4 billion
                        let gone = broken.is_some_and(|set| set.contains(&(k as u32)));
                        // Each member at its own radius; fastened joints are not drawn (G2.3).
                        let radius = frame.radii.get(k).copied().unwrap_or(frame.radius);
                        if radius > 0.0 {
                            out.beams
                                .push((p + q * *na, p + q * *nb, radius, 0.0, gone));
                        }
                    }
                }
            }
        }
        let asleep = c
            .receiver()
            .newest()
            .map_or(0, |s| s.states.values().filter(|s| s.asleep).count());
        out.counts = (c.kinds.len(), asleep, 0);
        out.time = c.drawn_at() / f64::from(TICK_RATE);
        out
    }
}

/// The bones of the fallen person whose avatar `id` is at `pelvis`: each segment's from its
/// entity (`Kind::Segment`), none until every one is in view.
fn fallen_bones(id: EntityId, pelvis: (DVec3, DQuat), c: &Connection) -> Option<Vec<Frame64>> {
    let poses = c.poses();
    let mut at = vec![None; bones::SEGMENT_NAMES.len()];
    at[0] = Some(pelvis);
    for (segment, kind) in &c.kinds {
        if let Kind::Segment { person, index, .. } = kind
            && *person == id
            && let (Some(slot), Some(pose)) = (at.get_mut(usize::from(*index)), poses.get(segment))
        {
            *slot = Some((pose.position, pose.orientation));
        }
    }
    let at: Option<Vec<(DVec3, DQuat)>> = at.into_iter().collect();
    Some(bones::fallen_bones(&at?))
}

/// A frame of `dt` seconds, `behind` seconds after the last tick: the time to each tick that
/// falls due in it (from the frame's start, then from the tick before), the time after the
/// last, and how far behind the next tick it ends. Each tick must come at its moment: ticked
/// at the frame's start and drawn from there, the avatar hung back by the time past the tick
/// for a frame each tick, a stutter the planted feet hide and the trunk shows (fixed after
/// G1.10). A frame of a slow machine catches up at most a quarter of a second.
fn frame_ticks(behind: f64, dt: f64) -> (Vec<f64>, f64, f64) {
    let tick = 1.0 / f64::from(TICK_RATE);
    let mut left = dt.clamp(0.0, 0.25);
    let mut behind = behind;
    let mut to_each = Vec::new();
    // (A tick due within rounding of the frame's end is due.)
    while behind + left >= tick - 1e-12 {
        let to = (tick - behind).min(left);
        to_each.push(to);
        left -= to;
        behind = 0.0;
    }
    (to_each, left, behind + left)
}

#[cfg(test)]
mod frame_tests {
    use super::frame_ticks;

    /// At 144 frames a second against 60 ticks: every tick comes once at its moment, and the
    /// time passed adds up to the frames'.
    #[test]
    fn each_tick_comes_at_its_moment_within_its_frame() {
        let (dt, tick) = (1.0 / 144.0, 1.0 / 60.0);
        let (mut behind, mut clock, mut ticks) = (0.0, 0.0, Vec::new());
        for _ in 0..1440 {
            let (to_each, after, next) = frame_ticks(behind, dt);
            for to in to_each {
                clock += to;
                ticks.push(clock);
            }
            clock += after;
            behind = next;
        }
        assert!((clock - 10.0).abs() < 1e-9, "{clock}");
        assert_eq!(ticks.len(), 600);
        for (k, t) in ticks.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let due = (k + 1) as f64 * tick;
            assert!((t - due).abs() < 1e-9, "tick {k} at {t}, due {due}");
        }
        // A slow frame catches up at most a quarter of a second.
        assert_eq!(frame_ticks(0.0, 2.0).0.len(), 15);
    }
}
