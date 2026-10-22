//! The physics sandbox in the client: its simulation thread, its window and its drawing.

use crate::demo::{DemoMeshes, Overlay};
use crate::physics_sandbox::{Look, PhysicsSandbox, PhysicsScene, PhysicsSnapshot};
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
const GRANITE: Material = Material::dielectric([0.55, 0.52, 0.50], 0.85);
const WHITE_PAINT: Material = Material::dielectric([0.9, 0.9, 0.88], 0.6);
const CANVAS: Material = Material::dielectric([0.85, 0.35, 0.12], 0.8);
/// Sea water seen from above: dark, smooth (its colour from within, as the sea pass has it).
const WATER: Material = Material::dielectric([0.02, 0.07, 0.11], 0.08);

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

    /// Starts the scene again (on the current data).
    pub fn restart(&mut self) {
        self.sim = spawn(self.scene, self.tick_rate_hz);
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
                ui.horizontal(|ui| {
                    ui.radio_value(overlay, Overlay::Utilisation, "show member utilisation");
                    ui.radio_value(overlay, Overlay::PartTemperature, "show temperatures");
                });
                for note in [&snapshot.heat, &snapshot.drive, &snapshot.fluids].into_iter().flatten() {
                    ui.label(note);
                }
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
    let same = previous.beams.len() == current.beams.len()
        && previous.boxes.len() == current.boxes.len()
        && previous.hulls.len() == current.hulls.len();
    let utilisation = overlay == Overlay::Utilisation;
    let temperatures = overlay == Overlay::PartTemperature;
    let shown = |t: f32| (temperatures && t.is_finite()).then_some(t);
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
        // A snapshot without temperatures (the planet's) shows members unheated.
        let heat = current.beam_heat.get(k).copied().unwrap_or(f32::NAN);
        scene.add(
            meshes.cylinder,
            Instance {
                frame: Frame64::new(
                    origin + (a + b) * 0.5,
                    DQuat::from_rotation_arc(DVec3::Y, along / length),
                ),
                scale,
                material: if broken {
                    BROKEN
                } else {
                    incandescent(STEEL, heat)
                },
                heat: utilisation.then_some(usage * 100.0).or(shown(heat)),
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
        // A snapshot without looks (the planet's) shows crates, without heat.
        let t = current.box_heat.get(k).copied().unwrap_or(f32::NAN);
        let look = current.box_look.get(k).copied().unwrap_or(Look::Crate);
        let (exposure, wear) = current.box_age.get(k).copied().unwrap_or((0.0, 0.0));
        let material = match look {
            Look::Crate => CRATE,
            Look::Steel => incandescent(STEEL, t),
            Look::Concrete => CONCRETE,
            Look::Radiator => WHITE_PAINT,
            Look::Canvas => CANVAS,
            Look::Water => WATER,
            Look::Colour([r, g, b]) => Material::dielectric(
                [
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                ],
                0.85,
            ),
        }
        .aged(exposure, wear);
        match meshes.models.crate_box {
            // The 1 m crate is drawn as its model, from the centre of its base (G1.3b).
            Some(model) if look == Look::Crate && is_crate(half) => scene.add(
                model,
                Instance {
                    frame: Frame64::new(origin + c - q * DVec3::Y * half.y, q),
                    scale: Vec3::ONE,
                    material,
                    heat: shown(t),
                },
            ),
            _ => add_box(scene, meshes, origin, (c, q, half), material, shown(t)),
        }
    }
    for (k, &(c, q, index)) in current.hulls.iter().enumerate() {
        let (c, q) = if same {
            (
                lerp(previous.hulls[k].0, c),
                previous.hulls[k].1.slerp(q, alpha),
            )
        } else {
            (c, q)
        };
        scene.add(
            meshes.hulls[index],
            Instance {
                frame: Frame64::new(origin + c, q),
                scale: Vec3::ONE,
                material: GRANITE,
                heat: None,
            },
        );
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
    if let Some(fire) = current.fire {
        fire_bed(scene, meshes, origin, fire);
    }
    for &wall in &current.walls {
        add_box(scene, meshes, origin, wall, CONCRETE, None);
    }
}

/// Whether a box of `half` extents is the 1 m crate the crate model draws (to a millimetre).
fn is_crate(half: DVec3) -> bool {
    (half - DVec3::splat(0.5)).abs().max_element() < 1e-3
}

fn add_box(
    scene: &mut Scene,
    meshes: &DemoMeshes,
    origin: DVec3,
    (c, q, half): (DVec3, DQuat, DVec3),
    material: Material,
    heat: Option<f32>,
) {
    scene.add(
        meshes.cube,
        Instance {
            frame: Frame64::new(origin + c, q),
            scale: (half * 2.0).as_vec3(),
            material,
            heat,
        },
    );
}

/// `base` glowing at `celsius`: steel shows a dull red from the Draper point (798 K), orange
/// by 900 °C, yellow-white past 1300 °C (blackbody colours, Charity's table); the
/// brightness rises steeply with temperature, as visible radiance does.
fn incandescent(base: Material, celsius: f32) -> Material {
    let t = celsius + 273.15;
    // Below the Draper point, or no temperature at all (NaN).
    if t.is_nan() || t <= 798.0 {
        return base;
    }
    let colour = |k: f32| {
        // Charity's blackbody sRGB: 1000 K #ff3800, 1500 K #ff6d00, 2000 K #ff8912.
        let f = ((k - 1000.0) / 1000.0).clamp(0.0, 1.0);
        [1.0, 0.22 + 0.32 * f, 0.07 * f * f]
    };
    let glow = ((t - 798.0) / 800.0).clamp(0.0, 1.0);
    Material::emissive(colour(t), 8.0 * glow * glow * glow.sqrt())
}

/// A fire's bed: its glow follows its gas temperature.
fn fire_bed(
    scene: &mut Scene,
    meshes: &DemoMeshes,
    origin: DVec3,
    (c, half, celsius): (DVec3, DVec3, f32),
) {
    let strength = (celsius / 900.0).clamp(0.0, 1.5) * 3.0;
    let material = Material::emissive([1.0, 0.42, 0.08], strength);
    add_box(
        scene,
        meshes,
        origin,
        (c, DQuat::IDENTITY, half),
        material,
        None,
    );
}
