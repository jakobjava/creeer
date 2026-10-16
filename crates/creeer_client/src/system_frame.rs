//! Building one frame of the solar-system sandbox (flight or map), shared by the window and
//! the headless screenshots.

use crate::DemoMeshes;
use crate::demo::Overlay;
use crate::physics_view;
use crate::system_map::{self, Label};
use crate::system_sim::SystemSnapshot;
use crate::system_view::{self, SystemView};
use creeer_core::Published;
use creeer_math::glam::DVec3;
use creeer_render::{Camera, DebugLines, Renderer, Scene, Sky};
use creeer_sim::universe::predict::PlotFrame;
use creeer_sim::universe::terrains::BodyRef;

/// What the frame needs besides the scene.
#[derive(Debug)]
pub struct Built {
    /// The camera to draw with.
    pub camera: Camera,
    /// Home's sky (flight view).
    pub sky: Option<Sky>,
    /// Towards the star (render space).
    pub sun: DVec3,
    /// Map labels.
    pub labels: Vec<Label>,
}

/// The plotting frame the map uses now.
pub fn plot_frame(view: &SystemView, s: &SystemSnapshot) -> PlotFrame {
    if view.corotating {
        let pair = match view.selected {
            Some(BodyRef::Major(i)) => s.bodies[i].parent.map(|p| (p, i)),
            _ => s.lagrange.map(|(p, q, _)| (p, q)),
        };
        if let Some((p, q)) = pair {
            return PlotFrame::Corotating(p, q);
        }
    }
    match (view.selected, &s.focus) {
        (Some(b), _) => PlotFrame::Body(b),
        (None, Some(f)) => PlotFrame::Body(BodyRef::Major(f.primary)),
        (None, None) => PlotFrame::Body(BodyRef::Major(0)),
    }
}

/// Index of the home planet among the bodies.
pub fn home(s: &SystemSnapshot) -> usize {
    s.bodies
        .iter()
        .position(|b| b.body.surface.as_deref() == Some("planets/home.ron"))
        .unwrap_or(0)
}

/// Builds the scene and lines of a frame; `camera` is the flight camera (already moved).
pub fn build(
    view: &mut SystemView,
    renderer: &mut Renderer,
    (scene, lines, meshes): (&mut Scene, &mut DebugLines, &DemoMeshes),
    camera: &Camera,
    published: &Published<SystemSnapshot>,
    alpha: f64,
) -> Built {
    let s = &published.current;
    let (bodies, pose) = SystemView::interpolate(published, alpha);
    scene.clear();
    lines.clear();
    if view.map {
        let frame = plot_frame(view, s);
        if s.prediction.0 != frame {
            view.send(crate::system_cmd::Command::Plot(Some(frame)));
        }
        let camera = system_map::camera(&view.map_camera);
        let labels = system_map::build(scene, lines, meshes, s, &bodies, (frame, camera.position));
        let sun = system_view::sun_direction(&bodies, camera.position, |x| {
            system_map::to_plot(frame, &bodies, x)
        });
        return Built {
            camera,
            sky: None,
            sun,
            labels,
        };
    }
    let to_render = |x: DVec3| pose.point_from_inertial(x);
    view.update_terrain(renderer, &s.terrains, &bodies, &pose, camera.position);
    let skip = (!view.draw.is_empty()).then(|| view.near_body()).flatten();
    system_view::draw_bodies(
        scene,
        meshes,
        &bodies,
        pose.rotation.conjugate(),
        to_render,
        skip,
    );
    let same = published.previous.generation == s.generation;
    physics_view::draw(
        scene,
        meshes,
        DVec3::ZERO,
        if same {
            (&published.previous.physics, &s.physics, alpha)
        } else {
            (&s.physics, &s.physics, 1.0)
        },
        Overlay::Off,
    );
    Built {
        camera: *camera,
        sky: Some(system_view::sky(&bodies, home(s), to_render)),
        sun: system_view::sun_direction(&bodies, camera.position, to_render),
        labels: Vec::new(),
    }
}

/// The surface nearest to `camera` (bubble coordinates): its centre in render space and the
/// height above its mean radius.
pub fn nearest_surface(s: &SystemSnapshot, camera: DVec3) -> Option<(DVec3, f64)> {
    let (cam, _) = s.pose.to_inertial(camera, DVec3::ZERO);
    s.bodies
        .iter()
        .filter(|b| b.body.kind.has_surface())
        .map(|b| {
            let height = (b.state.0 - cam).length() - b.body.radius;
            (s.pose.point_from_inertial(b.state.0), height)
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Light in space, away from home's atmosphere: the sun alone, a faint ambient so night
/// sides are not pure black, no shadows beyond the near field.
pub fn space_lighting() -> creeer_render::Lighting {
    creeer_render::Lighting {
        sky_ambient: [0.02, 0.02, 0.03],
        ground_ambient: [0.0; 3],
        shadow_distance: 250.0,
        ..creeer_render::Lighting::default()
    }
}
