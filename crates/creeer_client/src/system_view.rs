//! The F4 solar-system sandbox in the client (ADR-0007): the universe on its own simulation
//! thread, seen in flight (the physics bubble's frame) or on the map.
//!
//! In flight everything is drawn in the bubble's coordinates: the terrain of the body nearest
//! the camera in that body's moving, turning frame, every other body as a sphere, the sun
//! where the star really is (day and night follow from the orbits). When the bubble changes
//! frame the camera is carried along by the same change, so nothing jumps.

use crate::demo::DemoMeshes;
use crate::planet_view::render_mesh;
use crate::system_cmd::{Command, Engine};
use crate::system_sim::{BodyView, Commands, SystemSim, SystemSnapshot};
use creeer_core::{Published, SimThread};
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_orbit::BodyKind;
use creeer_render::{
    Atmosphere, Camera, Instance, Material, Renderer, Scene, Sky, TerrainFrame, TerrainMeshData,
};
use creeer_sim::universe::pose::Pose;
use creeer_sim::universe::terrains::{BodyRef, Terrains};
use creeer_terrain::Terrain;
use creeer_terrain::stream::{StreamSettings, Streamer};
use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

/// Bodies nearer than this many radii have their terrain streamed (m/m).
const TERRAIN_RADII: f64 = 30.0;
/// Brightness of the star's disc (times its colour).
const STAR_GLOW: f32 = 60.0;

/// The terrain of the body nearest the camera.
struct Near {
    which: BodyRef,
    terrain: Arc<Terrain>,
    streamer: Streamer<TerrainMeshData>,
    seen_edits: (u64, usize),
    uploaded: HashSet<u64>,
}

/// Where the map camera looks from (around the plotting frame's origin).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapCamera {
    /// Distance from the origin (m).
    pub distance: f64,
    /// Turn about the vertical (rad).
    pub yaw: f64,
    /// Height angle (rad).
    pub pitch: f64,
}

/// The running sandbox.
pub struct SystemView {
    pub(crate) sim: SimThread<SystemSim>,
    commands: Commands,
    near: Option<Near>,
    /// Terrain chunks to draw this frame.
    pub draw: Vec<(u64, u8)>,
    /// The frame they are drawn in.
    pub terrain_frames: Vec<TerrainFrame>,
    generation: u64,
    /// Showing the map instead of the flight view.
    pub map: bool,
    /// Map camera.
    pub map_camera: MapCamera,
    /// Map drawn turning with the focus's primary and its parent.
    pub corotating: bool,
    /// The flight camera follows the focused vessel at this offset (bubble axes).
    pub follow: Option<DVec3>,
    /// Body selected in the UI (map target, push target).
    pub selected: Option<BodyRef>,
    /// Engine settings edited in the UI.
    pub engine: Engine,
    /// Push tool: log10 of the force (N), direction, duration (s).
    pub push: (f64, crate::system_cmd::Direction, f64),
    /// Altitude for new ships (m).
    pub ship_altitude: f64,
}

impl std::fmt::Debug for SystemView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemView")
            .field("map", &self.map)
            .finish_non_exhaustive()
    }
}

impl SystemView {
    /// Starts the sandbox on a universe.
    pub fn new(universe: creeer_sim::Universe, tick_rate_hz: u32) -> Self {
        let (sim, commands) = SystemSim::new(universe);
        Self {
            sim: SimThread::spawn(sim, tick_rate_hz),
            commands,
            near: None,
            draw: Vec::new(),
            terrain_frames: Vec::new(),
            generation: 0,
            map: false,
            map_camera: MapCamera {
                distance: 3e7,
                yaw: 0.6,
                pitch: 0.5,
            },
            corotating: false,
            follow: None,
            selected: None,
            engine: Engine::default(),
            push: (12.0, crate::system_cmd::Direction::Prograde, 3600.0),
            ship_altitude: 120_000.0,
        }
    }

    /// Sends a command to the simulation.
    pub fn send(&self, command: Command) {
        self.commands
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(command);
    }

    /// The latest simulation states.
    pub fn snapshot(&self) -> Published<SystemSnapshot> {
        self.sim.latest()
    }

    /// Carries the flight camera through frame changes, and along with a followed vessel.
    pub fn update_camera(&mut self, camera: &mut Camera, s: &SystemSnapshot) {
        if s.generation != self.generation {
            if let Some(change) = s.change {
                camera.position = change.point(camera.position);
                camera.frame = (change.rotation * camera.frame).normalize();
            }
            self.generation = s.generation;
        }
        if let (Some(offset), Some(focus)) = (self.follow, &s.focus) {
            let (local, _) = s.pose.from_inertial(focus.state.0, focus.state.1);
            camera.position = local + offset;
        }
    }

    /// Bodies interpolated between two ticks (inertial), and the bubble's frame.
    pub fn interpolate(p: &Published<SystemSnapshot>, alpha: f64) -> (Vec<BodyView>, Pose) {
        let (a, b) = (&p.previous, &p.current);
        if a.generation != b.generation || a.bodies.len() != b.bodies.len() {
            return (b.bodies.to_vec(), b.pose);
        }
        let bodies = a
            .bodies
            .iter()
            .zip(b.bodies.iter())
            .map(|(x, y)| {
                let mut body = y.clone();
                body.state.0 = x.state.0.lerp(y.state.0, alpha);
                body.rotation = x.rotation.slerp(y.rotation, alpha);
                body
            })
            .collect();
        let pose = Pose {
            position: a.pose.position.lerp(b.pose.position, alpha),
            velocity: b.pose.velocity,
            rotation: a.pose.rotation.slerp(b.pose.rotation, alpha),
            spin: b.pose.spin,
        };
        (bodies, pose)
    }

    /// Streams the terrain of the body nearest the camera (`camera` in bubble coordinates).
    pub fn update_terrain(
        &mut self,
        renderer: &mut Renderer,
        terrains: &Arc<Mutex<Terrains>>,
        bodies: &[BodyView],
        pose: &Pose,
        camera: DVec3,
    ) {
        let (cam, _) = pose.to_inertial(camera, DVec3::ZERO);
        let nearest = bodies
            .iter()
            .filter(|b| b.body.kind.has_surface())
            .map(|b| {
                (
                    b,
                    ((b.state.0 - cam).length() - b.body.radius) / b.body.radius,
                )
            })
            .filter(|(_, d)| *d < TERRAIN_RADII)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(b, _)| b);
        if self.near.as_ref().map(|n| n.which) != nearest.map(|b| b.which) {
            if let Some(old) = self.near.take() {
                for id in old.uploaded {
                    renderer.remove_terrain(id);
                }
            }
            if let Some(body) = nearest {
                let terrain = terrains
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(body.which, &body.body);
                if let Some(terrain) = terrain {
                    let streamer =
                        Streamer::new(Arc::clone(&terrain), StreamSettings::default(), render_mesh);
                    self.near = Some(Near {
                        which: body.which,
                        terrain,
                        streamer,
                        seen_edits: (0, 0),
                        uploaded: HashSet::new(),
                    });
                }
            }
        }
        self.draw.clear();
        self.terrain_frames.clear();
        let Some(near) = &mut self.near else {
            return;
        };
        let Some(body) = bodies.iter().find(|b| b.which == near.which) else {
            return;
        };
        let log_len = near.terrain.edits().len();
        if log_len > near.seen_edits.1 {
            let new: Vec<_> = near.terrain.edits().log()[near.seen_edits.1..].to_vec();
            for edit in new {
                let (lo, hi) = edit.brush().bounds();
                near.streamer.invalidate(lo, hi);
            }
            near.seen_edits.1 = log_len;
        }
        let local = body.rotation.conjugate() * (cam - body.state.0);
        let update = near.streamer.update(local);
        for (key, mesh) in &update.ready {
            renderer.insert_terrain_in(key.id(), 0, key.origin(), mesh);
            near.uploaded.insert(key.id());
        }
        for key in &update.evicted {
            renderer.remove_terrain(key.id());
            near.uploaded.remove(&key.id());
        }
        self.draw
            .extend(update.draw.iter().map(|(key, caps)| (key.id(), *caps)));
        self.terrain_frames.push(TerrainFrame {
            position: pose.point_from_inertial(body.state.0),
            rotation: (pose.rotation.conjugate() * body.rotation).normalize(),
        });
    }

    /// The body whose terrain is drawn.
    pub fn near_body(&self) -> Option<BodyRef> {
        self.near.as_ref().map(|n| n.which)
    }

    /// Takes the snapshot's frame as the one the camera is in (after placing a camera in it).
    pub fn adopt_frame(&mut self, s: &SystemSnapshot) {
        self.generation = s.generation;
    }

    /// Terrain chunks still to come: queued or being built (0 when settled or without terrain).
    pub fn outstanding(&self) -> usize {
        self.near.as_ref().map_or(0, |n| n.streamer.outstanding())
    }
}

/// Every body as a sphere (in the coordinates `to_render` maps inertial positions to), except
/// `skip` (drawn as terrain).
pub fn draw_bodies(
    scene: &mut Scene,
    meshes: &DemoMeshes,
    bodies: &[BodyView],
    rotation: DQuat,
    to_render: impl Fn(DVec3) -> DVec3,
    skip: Option<BodyRef>,
) {
    for body in bodies {
        if Some(body.which) == skip {
            continue;
        }
        let color = body.body.color.map(|c| f32::from(c) / 255.0);
        let material = if body.body.kind == BodyKind::Star {
            Material::emissive(color, STAR_GLOW)
        } else {
            Material::dielectric(color, 0.9)
        };
        #[allow(clippy::cast_possible_truncation)] // radii fit f32
        let diameter = (2.0 * body.body.radius) as f32;
        scene.add(
            meshes.sphere,
            Instance {
                frame: creeer_math::Frame64::new(
                    to_render(body.state.0),
                    (rotation * body.rotation).normalize(),
                ),
                scale: Vec3::splat(diameter),
                material,
                heat: None,
            },
        );
    }
}

/// The home planet's sky at its position in render space.
pub fn sky(bodies: &[BodyView], home: usize, to_render: impl Fn(DVec3) -> DVec3) -> Sky {
    let home = &bodies[home];
    Sky {
        atmosphere: Atmosphere::earthlike(home.body.radius),
        planet_center: to_render(home.state.0),
        sun_irradiance: [3.6, 3.4, 3.1],
        star_brightness: 1.0,
    }
}

/// Unit vector towards the star from `camera` (render space).
pub fn sun_direction(
    bodies: &[BodyView],
    camera: DVec3,
    to_render: impl Fn(DVec3) -> DVec3,
) -> DVec3 {
    bodies
        .iter()
        .find(|b| b.body.kind == BodyKind::Star)
        .map_or(DVec3::Y, |star| {
            (to_render(star.state.0) - camera).normalize()
        })
}

impl SystemView {
    /// Digs a 2 m hole where the camera looks, on the body the bubble turns with.
    pub fn dig(&self, camera: &Camera, anchor: Option<BodyRef>) {
        let (Some(near), Some(anchor)) = (&self.near, anchor) else {
            return;
        };
        if near.which != anchor {
            return;
        }
        // In a surface frame, bubble coordinates are the body's own.
        let dir = camera.forward();
        let mut t = 0.0;
        while t < crate::planet_view::TOOL_REACH {
            let p = camera.position + dir * t;
            let d = near.terrain.density(p);
            if d < 0.02 {
                self.send(Command::Edit(creeer_terrain::edits::Edit::Dig(
                    creeer_terrain::edits::Brush::Sphere {
                        center: p,
                        radius: 2.0,
                    },
                )));
                return;
            }
            t += (0.35 * d).max(0.05);
        }
    }
}
