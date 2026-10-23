//! Play scenes (G1.0, `docs/g1-plan.md`): a server world and scripted players, played over
//! the real path (clients connected in process through counting links, their inputs and
//! commands sent as messages), headless. Each tick's time is the server's tick alone; its
//! work is the Newton iterations the world's bodies took; every client's bytes are counted
//! both ways; the end state is hashed (the same on every OS and every replay).
//!
//! Every play scene is gated (plan.md, Phase B): its worst tick and p99 in time
//! (`creeer_tools play --gate`, the development machine), its most work in a tick and its
//! bandwidth in CI (`play/tests.rs`).

mod lumberjack;
mod quarry;
pub mod record;
pub mod scenes;
mod toolmaking;
mod work;

use crate::client::{Connection, Status};
use crate::{Server, ServerWorld};
use creeer_math::glam::DVec3;
use creeer_net::link::{Conditions, Relay, SimulatedLink};
use creeer_terrain::Terrain;
use record::{Act, Recording};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A tick (s).
const STEP: Duration = Duration::from_nanos(16_666_667);
/// The most ticks players are given to join.
const JOINING: usize = 600;

/// A server and its scripted players, stepped in lockstep.
pub struct Play {
    /// The server.
    pub server: Server,
    players: Vec<(Connection, Relay)>,
    now: Duration,
    recording: Recording,
    /// The scene turned about the spawn by this angle (rad), an ensemble's member: the same
    /// script on slightly different ground (0: the scene itself, its hash the golden one).
    pub heading: f64,
}

impl std::fmt::Debug for Play {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Play")
            .field("players", &self.players.len())
            .field("ticks", &self.recording.ticks.len())
            .finish_non_exhaustive()
    }
}

/// What one tick cost.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tick {
    /// The server's tick (ms).
    pub ms: f64,
    /// Newton iterations the world's bodies took in it.
    pub work: u64,
    /// The physics' phases in the server's tick alone (`creeer_physics::profile`, with its
    /// `profile` feature; the clients' own prediction steps left out).
    pub phases: [u64; creeer_physics::profile::SLOTS],
}

fn iterations(server: &Server) -> u64 {
    (server.world.world.bodies.iter())
        .map(|b| b.solver.iterations_total)
        .sum()
}

/// FNV-1a over bytes.
pub fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

impl Play {
    /// `players` joining `world`, each through a lossless link that counts its bytes, their
    /// prediction on `terrain` (a client's own copy: as on another machine). Returns once
    /// every one has joined and its avatar stands still.
    ///
    /// # Panics
    /// If they have not joined within [`JOINING`] ticks.
    pub fn new(world: ServerWorld, scene: &str, players: &[&str], terrain: &Arc<Terrain>) -> Self {
        let content = world.data.hash();
        let play_data = std::sync::Arc::clone(&world.data);
        let mut server = Server::new(world);
        let joined = (players.iter().enumerate())
            .map(|(k, name)| {
                let end = server.connect_memory();
                let link = SimulatedLink::new(Conditions::default(), 7 + k as u64);
                let (relay, end) = Relay::new(end, link);
                let mut client = Connection::memory(end, name, content);
                // Each its own terrain, as on its own machine (a dig's round applied twice digs
                // deeper).
                client.attach_terrain(Arc::new(terrain.sibling()), &play_data);
                (client, relay)
            })
            .collect();
        let mut play = Self {
            server,
            players: joined,
            now: Duration::ZERO,
            recording: Recording {
                scene: scene.to_owned(),
                players: players.iter().map(|&p| p.to_owned()).collect(),
                ticks: Vec::new(),
            },
            heading: 0.0,
        };
        let still = vec![Act::default(); players.len()];
        for _ in 0..JOINING {
            if play
                .players
                .iter()
                .all(|(c, _)| c.status() == &Status::Joined)
            {
                for (client, _) in &mut play.players {
                    client.record();
                }
                play.recording.ticks.clear();
                return play;
            }
            play.tick(&still);
        }
        panic!("the players did not join within {JOINING} ticks");
    }

    /// One tick: every player's act sent, the server's tick, and what it sent handed over.
    ///
    /// # Panics
    /// If `acts` are not one per player.
    pub fn tick(&mut self, acts: &[Act]) -> Tick {
        assert_eq!(acts.len(), self.players.len(), "one act per player");
        for ((client, relay), act) in self.players.iter_mut().zip(acts) {
            relay.pump(self.now);
            client.update(STEP);
            for command in &act.commands {
                client.command(command.clone());
            }
            client.input(act.walk, act.jump, act.thrust);
            client.flush();
            relay.pump(self.now);
        }
        self.recording.ticks.push(acts.to_vec());
        let before = iterations(&self.server);
        let _ = creeer_physics::profile::take();
        let start = Instant::now();
        self.server.tick();
        let ms = start.elapsed().as_secs_f64() * 1e3;
        let phases = creeer_physics::profile::take();
        let work = iterations(&self.server).saturating_sub(before);
        for (_, relay) in &mut self.players {
            relay.pump(self.now);
        }
        self.now += STEP;
        Tick { ms, work, phases }
    }

    /// The players.
    pub fn players(&self) -> usize {
        self.players.len()
    }

    /// Player `k`'s connection (what its client knows).
    pub fn client(&self, k: usize) -> &Connection {
        &self.players[k].0
    }

    /// Time passing for player `k` between ticks, as its screen draws: its predicted avatar is
    /// drawn on between its last two ticks (what a client's `update` does each frame).
    pub fn draw_later(&mut self, k: usize, seconds: f64) {
        if let Some(p) = &mut self.players[k].0.predicted {
            p.advance(seconds);
        }
    }

    /// Where the server has player `k`'s avatar.
    ///
    /// # Panics
    /// If the player has not joined.
    pub fn avatar(&self, k: usize) -> DVec3 {
        let id = self.players[k].0.welcome.as_ref().expect("joined").avatar;
        let i = self.server.world.index(id).expect("in the world");
        self.server.world.world.bodies[i].structure.position[0]
    }

    /// The bytes sent to and from player `k` so far, as on the wire.
    pub fn bytes(&self, k: usize) -> (u64, u64) {
        let link = &self.players[k].1.link;
        (link.sent_to_client().0, link.sent_to_server().0)
    }

    /// What the players did since they joined.
    pub fn recording(&self) -> &Recording {
        &self.recording
    }

    /// Each player's own track, as its client recorded what it sent (`Connection::record`).
    pub fn tracks(&mut self) -> Vec<(String, Vec<Act>)> {
        (self.recording.players.iter().cloned())
            .zip(self.players.iter_mut().map(|(c, _)| c.take_recording()))
            .collect()
    }

    /// The hash of the world as it would be saved.
    pub fn hash(&mut self) -> u64 {
        self.server.world.join_machines();
        hash(&self.server.world.to_save().to_bytes())
    }
}

/// What a whole play cost.
#[derive(Debug)]
pub struct Report {
    /// Every tick's cost.
    pub ticks: Vec<Tick>,
    /// Each player's bytes (down, up) over the scene.
    pub bytes: Vec<(u64, u64)>,
    /// The end state's hash.
    pub hash: u64,
    /// What the players did.
    pub recording: Recording,
}

impl Report {
    /// The tick time at percentile `q` (0–100, ms).
    pub fn percentile_ms(&self, q: f64) -> f64 {
        let mut ms: Vec<f64> = self.ticks.iter().map(|t| t.ms).collect();
        ms.sort_by(f64::total_cmp);
        if ms.is_empty() {
            return f64::NAN;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = ((q / 100.0) * (ms.len() - 1) as f64).round() as usize;
        ms[i.min(ms.len() - 1)]
    }

    /// The most work in a tick, and its tick.
    pub fn most_work(&self) -> (usize, u64) {
        (self.ticks.iter().map(|t| t.work).enumerate())
            .max_by_key(|&(_, w)| w)
            .unwrap_or((0, 0))
    }

    /// The most bytes a second any one player was sent, and sent (kbit/s).
    pub fn kbits(&self) -> (f64, f64) {
        #[allow(clippy::cast_precision_loss)]
        let seconds = self.ticks.len() as f64 * STEP.as_secs_f64();
        let most = |f: fn(&(u64, u64)) -> u64| {
            #[allow(clippy::cast_precision_loss)]
            let m = self.bytes.iter().map(f).max().unwrap_or(0) as f64;
            m * 8e-3 / seconds.max(1e-9)
        };
        (most(|b| b.0), most(|b| b.1))
    }
}

/// Plays a recording in a world, from the moment its players have joined.
pub fn replay(world: ServerWorld, recording: &Recording, terrain: &Arc<Terrain>) -> Report {
    let names: Vec<&str> = recording.players.iter().map(String::as_str).collect();
    let mut play = Play::new(world, &recording.scene, &names, terrain);
    let start: Vec<_> = (0..play.players()).map(|k| play.bytes(k)).collect();
    let ticks = recording.ticks.iter().map(|acts| play.tick(acts)).collect();
    finish(play, ticks, &start)
}

fn finish(mut play: Play, ticks: Vec<Tick>, start: &[(u64, u64)]) -> Report {
    let bytes = (0..play.players())
        .map(|k| {
            let (d, u) = play.bytes(k);
            (d - start[k].0, u - start[k].1)
        })
        .collect();
    Report {
        ticks,
        bytes,
        hash: play.hash(),
        recording: play.recording.clone(),
    }
}

#[cfg(test)]
mod rest_probe;
#[cfg(test)]
mod rest_tests;
#[cfg(test)]
mod tests;
