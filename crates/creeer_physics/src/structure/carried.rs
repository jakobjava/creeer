//! What each member and joint of a structure carries now (G2.11, ADR-0039): read for the stress
//! overlay of creative servers and to tell, when something breaks, what it carried just before.
//! Nothing here changes a step.

use super::Structure;
use super::element::{Element, MEASURES};
use creeer_math::glam::DMat3;

/// What one element (a member or a fastened joint) carries.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Carried {
    /// Its two nodes.
    pub a: usize,
    pub b: usize,
    /// A fastened joint (else a member).
    pub joint: bool,
    pub broken: bool,
    /// How near it is to failing: 0 unloaded, 1 failing.
    /// - A member: its plastic capacity's use, as [`Structure::utilization`] (for brittle wood,
    ///   its strength's).
    /// - A joint: the most of its fasteners' pull-out interaction, (F_ax/R_ax)^e + (F_la/R_la)^e
    ///   (EN 1995-1-1 §8.3.3, as `Fastened::withdrawn` breaks it), and of its slip's way to its
    ///   last slip.
    pub share: f64,
    /// A member: its tension (N; compression negative). A joint: its fasteners' pull (N, apart
    /// positive).
    pub along: f64,
    /// A member: its largest bending moment at either end (N·m). A joint: its fasteners' slip
    /// force (N).
    pub across: f64,
    /// A joint: how far it has slipped past its peak towards its last slip (0–1).
    pub slipped: f64,
}

impl Structure {
    /// What each element carries now, in the elements' order.
    pub fn carried(&self) -> Vec<Carried> {
        (self.elements.iter())
            .map(|e| {
                let r = |node: usize| DMat3::from_quat(self.orientation[node]);
                let s = e.measures(self.position[e.a], r(e.a), self.position[e.b], r(e.b));
                let force = e.stiffness.apply(&s);
                let mut c = if e.is_joint() {
                    joint(e, &force)
                } else {
                    member(e, &force)
                };
                (c.a, c.b, c.broken) = (e.a, e.b, e.broken);
                c
            })
            .collect()
    }
}

/// A member's forces: its axial force and largest end moment, and its share of its capacity.
fn member(e: &Element, force: &[f64; MEASURES]) -> Carried {
    let caps = e.capacities(&[0.0; MEASURES], 1.0).map(|f| f.capacity);
    let n = force[0] / caps[0];
    let ends = (0..2).map(|end| {
        let (my, mz) = (
            force[2 + end] / caps[2 + end],
            force[4 + end] / caps[4 + end],
        );
        n * n + (my * my + mz * mz).sqrt()
    });
    let moment = (0..2)
        .map(|end| creeer_math::det::hypot(force[2 + end], force[4 + end]))
        .fold(0.0, f64::max);
    Carried {
        joint: false,
        share: if e.broken {
            1.0
        } else {
            ends.fold((force[1] / caps[1]).abs(), f64::max)
        },
        along: force[0],
        across: moment,
        ..Carried::default()
    }
}

/// A joint's forces: its fasteners' pull and slip force, and the share of what breaks it.
fn joint(e: &Element, force: &[f64; MEASURES]) -> Carried {
    let Some(f) = e.fastened else {
        return Carried::default();
    };
    // As `Fastened::withdrawn` measures them: each plane's shear its end moments' sum over the
    // length.
    let length = e.rest_length;
    let (vy, vz) = (
        (force[2] + force[3]) / length,
        (force[4] + force[5]) / length,
    );
    let (pull, slip) = (force[0], creeer_math::det::hypot(vy, vz));
    let x = f.interaction;
    let slipping = creeer_math::det::pow((slip / f.lateral).min(1.0), x);
    let pulling = creeer_math::det::pow(pull.max(0.0) / f.withdrawal, x);
    let elongation = e.material.elongation;
    let start = e.material.uniform_elongation.min(elongation);
    let slipped = if elongation > start {
        ((e.plastic_strain - start) / (elongation - start)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    Carried {
        joint: true,
        share: if e.broken {
            1.0
        } else {
            (pulling + slipping).max(slipped)
        },
        along: pull,
        across: slip,
        slipped,
        ..Carried::default()
    }
}
