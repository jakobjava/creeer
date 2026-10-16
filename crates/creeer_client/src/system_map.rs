//! The map view: the solar system at true scale in a plotting frame (ADR-0007).
//!
//! Centred on a body (inertial axes), each major body's osculating orbit about its parent is
//! drawn as an ellipse (the n-body truth wanders from it slowly) and the focused ship's
//! predicted path as computed. Turning with a pair of bodies, the five Lagrange points stand
//! still and paths near them show their real shapes (halo, horseshoe, tadpole).

use crate::demo::DemoMeshes;
use crate::system_sim::{BodyView, SystemSnapshot};
use crate::system_view::{MapCamera, draw_bodies};
use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3};
use creeer_orbit::BodyKind;
use creeer_orbit::analysis;
use creeer_orbit::kepler::Elements;
use creeer_render::{Camera, Color, DebugLines, Scene};
use creeer_sim::universe::predict::PlotFrame;
use creeer_sim::universe::terrains::BodyRef;

/// A text marker at a point of the map.
#[derive(Clone, Debug)]
pub struct Label {
    /// Where (map space).
    pub at: DVec3,
    /// Text.
    pub text: String,
    /// sRGB colour.
    pub color: [u8; 3],
}

/// Map-space position of an inertial point now.
pub fn to_plot(frame: PlotFrame, bodies: &[BodyView], x: DVec3) -> DVec3 {
    let find = |r: BodyRef| bodies.iter().find(|b| b.which == r);
    match frame {
        PlotFrame::Body(b) => find(b).map_or(x, |b| x - b.state.0),
        PlotFrame::Corotating(p, s) => match (find(BodyRef::Major(p)), find(BodyRef::Major(s))) {
            (Some(p), Some(s)) => analysis::to_corotating(x, p.state, s.state),
            _ => x,
        },
    }
}

/// The map camera: on a sphere about the origin, looking at it.
pub fn camera(map: &MapCamera) -> Camera {
    let (sy, cy) = det::sin_cos(map.yaw);
    let (sp, cp) = det::sin_cos(map.pitch);
    let direction = DVec3::new(cp * sy, sp, cp * cy);
    let mut camera = Camera::new(direction * map.distance);
    camera.frame = DQuat::IDENTITY;
    let forward = -direction;
    camera.yaw = det::atan2(-forward.x, -forward.z);
    camera.pitch = det::asin(forward.y.clamp(-0.999, 0.999));
    camera
}

const ORBIT: Color = [0.35, 0.45, 0.6, 1.0];
const PATH: Color = [0.3, 0.95, 1.0, 1.0];
const IMPACT: Color = [1.0, 0.35, 0.25, 1.0];

/// Builds the map: bodies, orbits, the predicted path, Lagrange points; returns the labels.
pub fn build(
    scene: &mut Scene,
    lines: &mut DebugLines,
    meshes: &DemoMeshes,
    s: &SystemSnapshot,
    bodies: &[BodyView],
    (frame, camera): (PlotFrame, DVec3),
) -> Vec<Label> {
    let plot = |x: DVec3| to_plot(frame, bodies, x);
    draw_bodies(scene, meshes, bodies, DQuat::IDENTITY, plot, None);
    let mut labels = Vec::new();
    let majors: Vec<&BodyView> = bodies
        .iter()
        .filter(|b| matches!(b.which, BodyRef::Major(_)))
        .collect();
    for body in &majors {
        labels.push(Label {
            at: plot(body.state.0),
            text: body.body.name.clone(),
            color: body.body.color,
        });
        // Osculating orbits make sense in body-centred frames only.
        if matches!(frame, PlotFrame::Corotating(..)) {
            continue;
        }
        let Some(parent) = body.parent.and_then(|p| majors.get(p)) else {
            continue;
        };
        if body.body.kind == BodyKind::Star {
            continue;
        }
        let mu = parent.body.mu + body.body.mu;
        let el = Elements::from_state(
            mu,
            body.state.0 - parent.state.0,
            body.state.1 - parent.state.1,
        );
        if el.e >= 1.0 {
            continue;
        }
        let centre = plot(parent.state.0);
        let mut last = None;
        for k in 0..=180 {
            let mut point = el;
            point.true_anomaly = std::f64::consts::TAU * f64::from(k) / 180.0;
            let p = centre + point.to_state().0;
            if let Some(q) = last {
                lines.line(q, p, ORBIT);
            }
            last = Some(p);
        }
    }
    // The predicted path, if it was drawn in this frame.
    let (path_frame, prediction) = &*s.prediction;
    if *path_frame == frame {
        let colour = if prediction.impact.is_some() {
            IMPACT
        } else {
            PATH
        };
        for pair in prediction.points.windows(2) {
            lines.line(pair[0], pair[1], colour);
        }
    }
    if let Some(focus) = &s.focus {
        labels.push(Label {
            at: plot(focus.state.0),
            text: format!("▲ {}", focus.name),
            color: [120, 255, 255],
        });
    }
    for (_, name, at, landed) in &s.stored {
        labels.push(Label {
            at: plot(*at),
            text: format!("{} {name}", if *landed { "▪" } else { "·" }),
            color: [200, 200, 200],
        });
    }
    if let Some((_, _, points)) = &s.lagrange {
        for (k, p) in points.iter().enumerate() {
            labels.push(Label {
                at: plot(*p),
                text: format!("L{}", k + 1),
                color: [255, 210, 90],
            });
        }
    }
    // Markers of constant size on screen, so nothing is lost at true scale.
    for label in &labels {
        let size = (label.at - camera).length() * 0.008;
        let [r, g, b] = label.color.map(|c| f32::from(c) / 255.0);
        lines.cross(label.at, size, [r, g, b, 1.0]);
    }
    labels
}
