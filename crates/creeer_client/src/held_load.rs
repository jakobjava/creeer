//! What a character's hands hold, as the animation reaches for it (G2.10, ADR-0036): the
//! thing as drawn (its pose, the box about its shape) and where the server says the hands hold
//! it, in one hand or two (`creeer_net::entity::Carried`).

use crate::animate::grasp::Load;
use creeer_math::Frame64;
use creeer_math::glam::DVec3;
use creeer_net::entity::{Carried, EntityId, Kind, Shape};
use creeer_net::interp::Pose;
use std::collections::BTreeMap;

/// The load `carried` drawn at its pose in `poses`, its shape from `kinds` (none if it is not
/// in view, or is no thing to hold).
pub fn load(
    carried: &Carried,
    kinds: &BTreeMap<EntityId, Kind>,
    poses: &BTreeMap<EntityId, Pose>,
) -> Option<Load> {
    let pose = poses.get(&carried.load)?;
    let (centre, half) = bounds(kinds.get(&carried.load)?)?;
    Some(Load {
        id: carried.load.0,
        frame: Frame64::new(pose.position, pose.orientation),
        centre,
        half,
        grip: carried.grip,
        one_hand: carried.one_hand,
    })
}

/// The box about a thing's shape in its own frame: its centre and half extents (m).
fn bounds(kind: &Kind) -> Option<(DVec3, DVec3)> {
    let about = |points: &mut dyn Iterator<Item = DVec3>, pad: f64| {
        let (lo, hi) = points.fold((DVec3::MAX, DVec3::MIN), |(lo, hi), p| {
            (lo.min(p), hi.max(p))
        });
        (lo.cmple(hi).all()).then(|| (0.5 * (lo + hi), 0.5 * (hi - lo) + DVec3::splat(pad)))
    };
    match kind {
        Kind::Body(Shape::Box(half), _) => Some((DVec3::ZERO, *half)),
        Kind::Body(Shape::Sphere(radius), _) => Some((DVec3::ZERO, DVec3::splat(*radius))),
        Kind::Body(Shape::Hull(corners), _) => about(&mut corners.iter().copied(), 0.0),
        Kind::Tree(part) => about(&mut part.hull.iter().copied(), 0.0),
        Kind::Structure(frame) => {
            let radius = frame.radii.iter().copied().fold(frame.radius, f64::max);
            about(&mut frame.nodes.iter().copied(), radius)
        }
        Kind::Character { .. } | Kind::Segment { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creeer_math::glam::DQuat;
    use creeer_net::entity::Matter;

    #[test]
    fn a_held_box_and_a_pole_are_bounded_by_their_shapes() {
        let matter = Matter {
            mass: 20.0,
            friction: 0.6,
            restitution: 0.1,
            surface: [1e9, 0.3, 0.002],
            predicted: false,
            colour: None,
        };
        let (id, pose) = (
            EntityId(4),
            Pose {
                position: DVec3::new(1.0, 2.0, 3.0),
                orientation: DQuat::from_rotation_y(0.3),
            },
        );
        let poses = BTreeMap::from([(id, pose)]);
        let held = Carried::new(id, DVec3::new(0.0, 0.2, 0.0), false);
        let kinds = BTreeMap::from([(id, Kind::Body(Shape::Box(DVec3::splat(0.2)), matter))]);
        let got = super::load(&held, &kinds, &poses).expect("in view");
        assert_eq!((got.centre, got.half), (DVec3::ZERO, DVec3::splat(0.2)));
        assert_eq!(got.frame.position, pose.position);
        // A pole's hull: off its node, 3 m along x and 12 cm round.
        let corners = vec![DVec3::new(-1.4, -0.06, -0.06), DVec3::new(1.6, 0.06, 0.06)];
        let kinds = BTreeMap::from([(id, Kind::Body(Shape::Hull(corners), matter))]);
        let got = super::load(&held, &kinds, &poses).expect("in view");
        assert!((got.centre - DVec3::new(0.1, 0.0, 0.0)).length() < 1e-12);
        assert!((got.half - DVec3::new(1.5, 0.06, 0.06)).length() < 1e-12);
        // Out of view: nothing to reach for.
        assert!(super::load(&held, &kinds, &BTreeMap::new()).is_none());
    }
}
