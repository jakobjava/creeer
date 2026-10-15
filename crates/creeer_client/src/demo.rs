//! Tech-sandbox scene content (F1): an orbiting test system and, optionally, a "factory" of
//! 2,500 machines × 4 parts = 10,000 instanced parts for the renderer budget.

use crate::sandbox::SandboxSnapshot;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_math::{Frame64, det};
use creeer_render::{
    Atmosphere, Camera, Color, DebugLines, Heatmap, Instance, Material, MeshArena, MeshData,
    MeshId, Palette, Scene, Sky,
};
use std::f64::consts::FRAC_PI_2;

const AXIS_X: Color = [0.85, 0.30, 0.30, 1.0];
const AXIS_Y: Color = [0.35, 0.80, 0.35, 1.0];
const AXIS_Z: Color = [0.35, 0.50, 0.95, 1.0];

// Factorio-like colour language: strong, readable colours per material.
const GROUND: Material = Material::dielectric([0.42, 0.46, 0.34], 0.95);
const CONCRETE: Material = Material::dielectric([0.62, 0.61, 0.58], 0.9);
const STEEL: Material = Material::metal([0.66, 0.67, 0.69], 0.4);
const COPPER: Material = Material::metal([0.95, 0.62, 0.48], 0.35);
const DARK_IRON: Material = Material::metal([0.30, 0.30, 0.32], 0.55);
const PAINTS: [Material; 4] = [
    Material::dielectric([0.90, 0.68, 0.16], 0.5), // safety yellow
    Material::dielectric([0.30, 0.45, 0.62], 0.5), // machine blue
    Material::dielectric([0.72, 0.22, 0.17], 0.5), // signal red
    Material::dielectric([0.32, 0.56, 0.32], 0.5), // utility green
];
const SUN_BODY: Material = Material::dielectric([1.0, 0.82, 0.25], 0.6);
const PLANETOID: Material = Material::dielectric([0.85, 0.86, 0.90], 0.7);

/// Machines per side of the factory grid.
pub const FACTORY_SIDE: usize = 50;
/// Parts per machine.
#[cfg(test)]
pub const PARTS_PER_MACHINE: usize = 4;

/// Meshes used by the demo, registered once.
#[derive(Clone, Copy, Debug)]
pub struct DemoMeshes {
    cube: MeshId,
    cylinder: MeshId,
    sphere: MeshId,
}

impl DemoMeshes {
    /// Registers unit-sized primitives (1 m across) in the renderer's arena.
    pub fn register(arena: &mut MeshArena) -> Self {
        Self {
            cube: arena.add(&MeshData::cuboid(Vec3::splat(0.5))),
            cylinder: arena.add(&MeshData::cylinder(0.5, 0.5, 24)),
            sphere: arena.add(&MeshData::uv_sphere(0.5, 32, 16)),
        }
    }
}

/// Orbit system state to draw, interpolated between two simulation ticks.
#[derive(Clone, Copy, Debug)]
pub struct OrbitView<'a> {
    pub previous: &'a SandboxSnapshot,
    pub current: &'a SandboxSnapshot,
    pub alpha: f64,
}

/// Per-frame inputs of [`build`].
#[derive(Clone, Copy, Debug)]
pub struct DemoFrame<'a> {
    /// Where the sandbox sits in simulation space.
    pub origin: DVec3,
    /// Animation time (s).
    pub time: f64,
    /// Orbit system to draw.
    pub orbits: OrbitView<'a>,
    /// Whether to add the 10,000-part factory.
    pub factory: bool,
    /// Engineering overlay values to attach.
    pub overlay: Overlay,
}

/// Fills `scene` and `lines` for one frame.
pub fn build(
    scene: &mut Scene,
    lines: &mut DebugLines,
    meshes: &DemoMeshes,
    frame: &DemoFrame<'_>,
) {
    let DemoFrame {
        origin,
        time,
        orbits,
        factory,
        overlay,
    } = *frame;
    scene.clear();
    lines.clear();

    let place = |offset: DVec3, scale: Vec3, material: Material| Instance {
        frame: Frame64::from_position(origin + offset),
        scale,
        material,
        heat: None,
    };

    // Ground slab covering the orbit area and the factory.
    scene.add(
        meshes.cube,
        place(
            DVec3::new(110.0, -0.1, 0.0),
            Vec3::new(320.0, 0.2, 260.0),
            GROUND,
        ),
    );
    lines.line(origin, origin + DVec3::X * 5.0, AXIS_X);
    lines.line(origin, origin + DVec3::Y * 5.0, AXIS_Y);
    lines.line(origin, origin + DVec3::Z * 5.0, AXIS_Z);

    // Orbit system: a central mass and its test bodies.
    let center = DVec3::Y * 12.0;
    scene.add(meshes.sphere, place(center, Vec3::splat(2.0), SUN_BODY));
    for (a, b) in orbits
        .previous
        .positions
        .iter()
        .zip(&orbits.current.positions)
    {
        scene.add(
            meshes.sphere,
            place(
                center + a.lerp(*b, orbits.alpha),
                Vec3::splat(0.7),
                PLANETOID,
            ),
        );
    }
    // A concrete plinth under the orbit system.
    scene.add(
        meshes.cylinder,
        place(DVec3::Y * 0.25, Vec3::new(8.0, 0.5, 8.0), CONCRETE),
    );

    if factory {
        add_factory(scene, meshes, origin, time, overlay);
    }
}

fn add_factory(scene: &mut Scene, meshes: &DemoMeshes, origin: DVec3, time: f64, overlay: Overlay) {
    let spacing = 4.0;
    for i in 0..FACTORY_SIDE {
        for j in 0..FACTORY_SIDE {
            #[allow(clippy::cast_precision_loss)]
            let (fi, fj) = (i as f64, j as f64);
            let base = origin + DVec3::new(45.0 + fi * spacing, 0.0, -98.0 + fj * spacing);
            let paint = PAINTS[(i / 5 + j / 5) % PAINTS.len()];
            let at = |offset: DVec3| Frame64::from_position(base + offset);
            let angle = time * 2.0 + fi * 0.37 + fj * 0.61;
            let (temperature, stress) = (tank_temperature(fi, fj, time), crank_stress(angle));
            let shown = |kind: Overlay, value: f32| (overlay == kind).then_some(value);
            let temp = |value: f32| shown(Overlay::Temperature, value);

            scene.add(
                meshes.cube,
                Instance {
                    frame: at(DVec3::new(0.0, 0.3, 0.0)),
                    scale: Vec3::new(3.0, 0.6, 3.0),
                    material: paint,
                    heat: temp(20.0 + 0.3 * (temperature - 20.0)),
                },
            );
            scene.add(
                meshes.cylinder,
                Instance {
                    frame: at(DVec3::new(-0.6, 1.4, -0.6)),
                    scale: Vec3::new(1.2, 1.6, 1.2),
                    material: STEEL,
                    heat: temp(temperature),
                },
            );
            scene.add(
                meshes.sphere,
                Instance {
                    frame: at(DVec3::new(-0.6, 2.2, -0.6)),
                    scale: Vec3::splat(1.1),
                    material: COPPER,
                    heat: temp(0.9 * temperature),
                },
            );
            // A crank arm spinning about the machine's Z axis, phase-shifted per machine.
            scene.add(
                meshes.cube,
                Instance {
                    frame: Frame64::new(
                        base + DVec3::new(0.8, 1.3, 0.8),
                        DQuat::from_rotation_z(angle),
                    ),
                    scale: Vec3::new(1.6, 0.18, 0.18),
                    material: DARK_IRON,
                    heat: temp(35.0).or(shown(Overlay::AxialStress, stress)),
                },
            );
        }
    }
}

/// Which engineering overlay the demo shows. The values are stand-ins until the physics
/// (F2, F6) produces real ones; they exercise the overlay pipeline end to end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overlay {
    /// Normal materials.
    #[default]
    Off,
    /// Machine temperatures (°C), sequential scale.
    Temperature,
    /// Axial stress in the crank arms (MPa): tension positive, compression negative.
    AxialStress,
}

impl Overlay {
    /// Scale and palette, or `None` when the overlay is off.
    pub fn heatmap(self) -> Option<Heatmap> {
        match self {
            Self::Off => None,
            Self::Temperature => Some(Heatmap {
                min: 20.0,
                max: 400.0,
                palette: Palette::Sequential,
            }),
            Self::AxialStress => Some(Heatmap {
                min: -120.0,
                max: 120.0,
                palette: Palette::Diverging,
            }),
        }
    }

    /// Legend title and unit.
    pub fn label(self) -> (&'static str, &'static str) {
        match self {
            Self::Off => ("", ""),
            Self::Temperature => ("Temperature", "°C"),
            Self::AxialStress => ("Axial stress", "MPa"),
        }
    }
}

/// Stand-in temperature field: slowly drifting hot spots between 20 and 400 °C.
fn tank_temperature(i: f64, j: f64, time: f64) -> f32 {
    let a = 0.5 + 0.5 * det::sin(i * 0.21 + time * 0.4);
    let b = 0.5 + 0.5 * det::cos(j * 0.17 - time * 0.25);
    #[allow(clippy::cast_possible_truncation)] // temperatures are small
    let t = (20.0 + 380.0 * a * b) as f32;
    t
}

/// Stand-in stress in a rotating crank: alternates between tension and compression.
fn crank_stress(angle: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let s = (120.0 * det::sin(angle)) as f32;
    s
}

/// Radius of the sandbox planet (m); the sandbox sits on its surface.
pub const PLANET_RADIUS: f64 = 500_000.0;

/// The sandbox planet with an Earth-like atmosphere, below the scene at `origin`.
pub fn sky(origin: DVec3) -> Sky {
    Sky {
        atmosphere: Atmosphere::earthlike(PLANET_RADIUS),
        planet_center: origin - DVec3::Y * PLANET_RADIUS,
        sun_irradiance: [3.6, 3.4, 3.1],
        star_brightness: 1.0,
    }
}

/// Unit vector towards a sun at `elevation` degrees above the sandbox horizon.
pub fn sun_direction(elevation: f64) -> DVec3 {
    let (sin_e, cos_e) = creeer_math::det::sin_cos(elevation.to_radians());
    // Fixed azimuth so shadows fall diagonally across the factory grid.
    let (sin_a, cos_a) = creeer_math::det::sin_cos(35f64.to_radians());
    DVec3::new(cos_e * cos_a, sin_e, cos_e * sin_a)
}

/// Named camera viewpoints for screenshots, relative to the scene origin.
pub fn camera_preset(name: &str, origin: DVec3) -> Option<Camera> {
    let (position, yaw, pitch) = match name {
        "overview" => (DVec3::new(-20.0, 35.0, 70.0), -0.55, -0.32),
        "factory" => (DVec3::new(30.0, 12.0, 30.0), -0.95, -0.25),
        "closeup" => (DVec3::new(48.0, 3.0, -86.0), -0.4, -0.12),
        "top" => (DVec3::new(140.0, 260.0, 0.0), 0.0, -FRAC_PI_2 + 0.02),
        // Factory seen from ~100 m: shadows come from the coarse cascades.
        "far" => (DVec3::new(-20.0, 40.0, 30.0), -1.4, -0.22),
        // 830 km up, looking down at the planet limb.
        "orbit" => (DVec3::new(0.0, 250_000.0, 1_100_000.0), 0.0, -0.598),
        // Standing on the ground looking at the horizon.
        "horizon" => (DVec3::new(-30.0, 1.8, 40.0), 0.6, 0.06),
        _ => return None,
    };
    let mut camera = Camera::new(origin + position);
    camera.yaw = yaw;
    camera.pitch = pitch;
    Some(camera)
}

/// Total instances `build` creates.
#[cfg(test)]
pub fn instance_count(bodies: usize, factory: bool) -> usize {
    3 + bodies
        + if factory {
            FACTORY_SIDE * FACTORY_SIDE * PARTS_PER_MACHINE
        } else {
            0
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::OrbitSandbox;
    use creeer_core::Simulation;

    #[test]
    fn factory_is_ten_thousand_parts() {
        assert_eq!(FACTORY_SIDE * FACTORY_SIDE * PARTS_PER_MACHINE, 10_000);
        let mut arena = MeshArena::default();
        let meshes = DemoMeshes::register(&mut arena);
        let sim = OrbitSandbox::demo();
        let snapshot = sim.snapshot();
        let view = OrbitView {
            previous: &snapshot,
            current: &snapshot,
            alpha: 0.0,
        };
        let (mut scene, mut lines) = (Scene::default(), DebugLines::default());
        let demo = DemoFrame {
            origin: DVec3::ZERO,
            time: 0.0,
            orbits: view,
            factory: true,
            overlay: Overlay::Temperature,
        };
        build(&mut scene, &mut lines, &meshes, &demo);
        assert_eq!(
            scene.instance_count(),
            instance_count(snapshot.positions.len(), true)
        );
        assert!(scene.instance_count() > 10_000);
    }

    #[test]
    fn presets_exist() {
        for name in [
            "overview", "factory", "closeup", "top", "far", "orbit", "horizon",
        ] {
            assert!(camera_preset(name, DVec3::ZERO).is_some(), "{name}");
        }
        assert!(camera_preset("nope", DVec3::ZERO).is_none());
    }
}
