//! The planet flyover (F7.5): the camera from orbit to the ground, then along it at 300 m/s,
//! the terrain streamed on its workers in real time (each frame's update timed).
//!
//! **No hitch** is measured as the detail under the camera: each frame, the level the
//! streamer would draw there with every chunk built (its split rule) against the level it
//! draws. A frame is behind while the drawn level is coarser; the longest such wait, in
//! real time, is reported with the chunks built per second.

use crate::stress_scenes::{Notes, Scene, hash};
use creeer_data::Data;
use creeer_math::glam::{DQuat, DVec3};
use creeer_server::world::load_terrain;
use creeer_terrain::chunk::{ChunkKey, MAX_CHUNK_LEVEL};
use creeer_terrain::stream::{StreamSettings, Streamer};
use creeer_terrain::terrain::Terrain;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Speed along the ground (m/s) and the camera's height above it (m).
const SPEED: f64 = 300.0;
const EYE: f64 = 2.0;
/// A frame of the flight (s).
const FRAME: f64 = 1.0 / 60.0;

struct Flight {
    streamer: Streamer<usize>,
    started: Instant,
    built: usize,
    /// Since when the detail under the camera is behind, and the longest wait.
    behind: Option<Instant>,
    longest: Duration,
    frames_behind: u32,
}

/// The level the streamer draws at `ground` from `camera` with every chunk built.
fn wanted(camera: DVec3, ground: DVec3, detail: f64) -> u8 {
    (1..=MAX_CHUNK_LEVEL)
        .rev()
        .find(|&level| {
            let key = ChunkKey::containing(ground, level);
            key.distance_to(camera) >= detail * key.size()
        })
        .unwrap_or(0)
}

/// Where the camera is at `frame`: descending for the first half of the flight, then along
/// the ground.
fn camera(terrain: &Terrain, up: DVec3, frame: u32, frames: u32) -> (DVec3, DVec3) {
    let radius = terrain.planet().radius;
    let half = (frames / 2).max(1);
    let (top, bottom) = (1_100_000.0f64, EYE);
    let (dir, altitude) = if frame < half {
        let t = f64::from(frame) / f64::from(half);
        (up, top * creeer_math::det::pow(bottom / top, t))
    } else {
        let along = SPEED * FRAME * f64::from(frame - half);
        let axis = up.any_orthonormal_vector();
        (DQuat::from_axis_angle(axis, along / radius) * up, bottom)
    };
    let datum = &terrain.planet().datum;
    let ground = dir * (datum.radius(dir) + terrain.surface_height(dir));
    (ground + dir * altitude, ground)
}

/// The flyover in `frames` frames. The hash is of fixed chunks under the start at every
/// level (what is streamed when depends on the workers' timing; what a chunk is must not).
pub fn flyover(frames: u32) -> Scene {
    let data = Data::base();
    let terrain = load_terrain(&data, "planets/home.ron").expect("home.ron");
    let up = creeer_terrain::elevation::fibonacci_sphere(5000)
        .find(|&d| (300.0..1500.0).contains(&terrain.surface_height(d)))
        .expect("land");
    let settings = StreamSettings::default();
    let streamer = Streamer::new(Arc::clone(&terrain), settings, |t, k| {
        t.mesh_chunk(k).map(|m| m.triangle_count())
    });
    let flight = Arc::new(Mutex::new(Flight {
        streamer,
        started: Instant::now(),
        built: 0,
        behind: None,
        longest: Duration::ZERO,
        frames_behind: 0,
    }));
    let (seen, at) = (Arc::clone(&flight), Arc::clone(&terrain));
    Scene {
        tick: Box::new(move |frame| {
            let mut f = flight.lock().expect("flight");
            if frame == 0 {
                f.started = Instant::now();
            }
            let (eye, ground) = camera(&at, up, frame, frames);
            let update = f.streamer.update(eye);
            f.built += update.ready.len();
            let drawn = update
                .draw
                .iter()
                .filter(|(k, _)| k.distance_to(ground) == 0.0)
                .map(|(k, _)| k.level)
                .min()
                .unwrap_or(MAX_CHUNK_LEVEL);
            if drawn > wanted(eye, ground, settings.detail) {
                f.frames_behind += 1;
                let since = *f.behind.get_or_insert_with(Instant::now);
                f.longest = f.longest.max(since.elapsed());
            } else {
                f.behind = None;
            }
        }),
        hash: Box::new(move || {
            let surface = up * (terrain.planet().datum.radius(up) + terrain.surface_height(up));
            hash((0..7u8).flat_map(|level| {
                let mesh = terrain.mesh_chunk(ChunkKey::containing(surface, level));
                mesh.map(|m| {
                    let p = m
                        .positions
                        .iter()
                        .flat_map(|v| v.to_array())
                        .flat_map(f32::to_le_bytes);
                    let i = m.indices.iter().flat_map(|i| i.to_le_bytes());
                    p.chain(i).collect::<Vec<u8>>()
                })
                .unwrap_or_default()
            }))
        }),
        notes: Box::new(move || {
            let f = seen.lock().expect("flight");
            let seconds = f.started.elapsed().as_secs_f64();
            let wait = f.longest.as_secs_f64() * 1e3;
            let text = format!(
                "{} meshes built ({:.0} a second), {} cached; detail behind in {} frames, longest wait {:.0} ms",
                f.built,
                f.built as f64 / seconds,
                f.streamer.cached(),
                f.frames_behind,
                wait
            );
            // (Each finer level is wanted a frame before it can be built; the start, with
            // nothing built, waits longest: 50 ms measured.)
            Notes {
                text,
                gauges: vec![("wait_ms", wait, 70.0)],
            }
        }),
        paced: true,
    }
}
