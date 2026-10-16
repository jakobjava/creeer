//! The physics sandbox in the client: its simulation thread, its window and its drawing.

use crate::demo::{DemoMeshes, Overlay};
use crate::physics_sandbox::{PhysicsSandbox, PhysicsScene, PhysicsSnapshot};
use creeer_core::SimThread;
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_render::{Instance, Material, Scene};
use creeer_ui::egui;
use std::time::Instant;

/// Where the physics scenes sit, relative to the sandbox origin.
pub const PHYSICS_OFFSET: DVec3 = DVec3::new(-80.0, 0.0, 0.0);

const STEEL: Material = Material::metal([0.66, 0.67, 0.69], 0.4);
const CRATE: Material = Material::dielectric([0.72, 0.52, 0.30], 0.8);
const BALL: Material = Material::metal([0.30, 0.30, 0.32], 0.5);
const CONCRETE: Material = Material::dielectric([0.62, 0.61, 0.58], 0.9);
const BROKEN: Material = Material::dielectric([0.15, 0.15, 0.15], 0.9);

/// The running physics scene.
pub struct PhysicsView {
    sim: SimThread<PhysicsSandbox>,
    scene: PhysicsScene,
    tick_rate_hz: u32,
}

impl std::fmt::Debug for PhysicsView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsView")
            .field("scene", &self.scene)
            .finish_non_exhaustive()
    }
}

impl PhysicsView {
    /// Starts the bridge scene.
    pub fn new(tick_rate_hz: u32) -> Self {
        Self {
            sim: spawn(PhysicsScene::Bridge, tick_rate_hz),
            scene: PhysicsScene::Bridge,
            tick_rate_hz,
        }
    }

    /// The scene picker and live numbers.
    pub fn window(&mut self, ctx: &egui::Context, overlay: &mut Overlay) {
        let snapshot = self.sim.latest().current;
        egui::Window::new("Physics")
            .default_pos([12.0, 640.0])
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (scene, name) in PhysicsScene::ALL {
                        if ui.radio(self.scene == scene, name).clicked() {
                            self.scene = scene;
                            self.sim = spawn(scene, self.tick_rate_hz);
                        }
                    }
                });
                if ui.button("restart").clicked() {
                    self.sim = spawn(self.scene, self.tick_rate_hz);
                }
                ui.radio_value(overlay, Overlay::Utilisation, "show member utilisation");
                let (bodies, asleep, contacts) = snapshot.counts;
                ui.label(format!(
                    "t = {:.1} s · {bodies} bodies ({asleep} asleep) · {contacts} contacts · step {:.2} ms",
                    snapshot.time, snapshot.step_ms
                ));
            });
    }

    /// Adds the scene, interpolated to `now`, to the frame.
    pub fn draw(
        &self,
        scene: &mut Scene,
        meshes: &DemoMeshes,
        origin: DVec3,
        now: Instant,
        overlay: Overlay,
    ) {
        let published = self.sim.latest();
        let alpha = published.alpha(now);
        draw(
            scene,
            meshes,
            origin + PHYSICS_OFFSET,
            (&published.previous, &published.current, alpha),
            overlay,
        );
    }
}

fn spawn(scene: PhysicsScene, tick_rate_hz: u32) -> SimThread<PhysicsSandbox> {
    SimThread::spawn(
        PhysicsSandbox::new(scene, 1.0 / f64::from(tick_rate_hz)),
        tick_rate_hz,
    )
}

/// Adds a snapshot pair (previous, current, blend) to `scene` at `origin`.
pub fn draw(
    scene: &mut Scene,
    meshes: &DemoMeshes,
    origin: DVec3,
    (previous, current, alpha): (&PhysicsSnapshot, &PhysicsSnapshot, f64),
    overlay: Overlay,
) {
    // Interpolate when both ticks have the same contents (always, within a scene).
    let lerp = |a: DVec3, b: DVec3| a.lerp(b, alpha);
    let same =
        previous.beams.len() == current.beams.len() && previous.boxes.len() == current.boxes.len();
    let utilisation = overlay == Overlay::Utilisation;
    for (k, &(a, b, radius, usage, broken)) in current.beams.iter().enumerate() {
        let (a, b) = if same {
            (lerp(previous.beams[k].0, a), lerp(previous.beams[k].1, b))
        } else {
            (a, b)
        };
        let along = b - a;
        let length = along.length();
        if length < 1e-9 {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)] // render-space sizes
        let scale = Vec3::new(2.0 * radius as f32, length as f32, 2.0 * radius as f32);
        scene.add(
            meshes.cylinder,
            Instance {
                frame: Frame64::new(
                    origin + (a + b) * 0.5,
                    DQuat::from_rotation_arc(DVec3::Y, along / length),
                ),
                scale,
                material: if broken { BROKEN } else { STEEL },
                heat: utilisation.then_some(usage * 100.0),
            },
        );
    }
    for (k, &(c, q, half)) in current.boxes.iter().enumerate() {
        let (c, q) = if same {
            (
                lerp(previous.boxes[k].0, c),
                previous.boxes[k].1.slerp(q, alpha),
            )
        } else {
            (c, q)
        };
        add_box(scene, meshes, origin, (c, q, half), CRATE);
    }
    for &(c, radius) in &current.spheres {
        #[allow(clippy::cast_possible_truncation)]
        let scale = Vec3::splat(2.0 * radius as f32);
        scene.add(
            meshes.sphere,
            Instance {
                frame: Frame64::from_position(origin + c),
                scale,
                material: BALL,
                heat: None,
            },
        );
    }
    for &wall in &current.walls {
        add_box(scene, meshes, origin, wall, CONCRETE);
    }
}

fn add_box(
    scene: &mut Scene,
    meshes: &DemoMeshes,
    origin: DVec3,
    (c, q, half): (DVec3, DQuat, DVec3),
    material: Material,
) {
    scene.add(
        meshes.cube,
        Instance {
            frame: Frame64::new(origin + c, q),
            scale: (half * 2.0).as_vec3(),
            material,
            heat: None,
        },
    );
}
