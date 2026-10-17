//! Collision shapes.

use super::hull::Hull;
use super::surface::Feature;
use creeer_math::glam::{DMat3, DQuat, DVec3};
use std::sync::Arc;

/// A shape attached to a node of a body, in the node's frame (centred on the node).
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A ball.
    Sphere {
        /// Radius (m).
        radius: f64,
    },
    /// A box aligned with the node's axes.
    Box {
        /// Half the edge lengths (m).
        half: DVec3,
    },
    /// A convex hull, centred on its centroid (`hull.rs`).
    Hull(Arc<Hull>),
}

impl Shape {
    /// Radius of a ball around the node that contains the shape.
    pub fn bounding_radius(&self) -> f64 {
        match self {
            Self::Sphere { radius } => *radius,
            Self::Box { half } => half.length(),
            Self::Hull(hull) => hull.bounding_radius(),
        }
    }

    /// Rotational inertia (kg·m², about the centre) of a solid body of this shape.
    pub fn inertia(&self, mass: f64) -> DMat3 {
        match self {
            Self::Sphere { radius } => DMat3::IDENTITY * (0.4 * mass * radius * radius),
            Self::Box { half } => {
                let s = *half * *half * 4.0;
                DMat3::from_diagonal(DVec3::new(s.y + s.z, s.x + s.z, s.x + s.y) * (mass / 12.0))
            }
            Self::Hull(hull) => hull.inertia(mass),
        }
    }
}

/// Fixed geometry of the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StaticShape {
    /// The half-space below the plane n·x = offset (solid where n·x < offset).
    Plane {
        /// Unit normal, pointing out of the solid.
        normal: DVec3,
        /// n·x on the surface (m).
        offset: f64,
    },
    /// A fixed box.
    Box {
        /// Centre (m).
        center: DVec3,
        /// Orientation.
        rotation: DQuat,
        /// Half the edge lengths (m).
        half: DVec3,
    },
}

/// Where a shape is: position and orientation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    /// Centre (m).
    pub position: DVec3,
    /// Orientation.
    pub rotation: DQuat,
}

/// How a contact's normal turns within the step: with the body whose face or feature it is
/// normal to (a box under a ball, a patch's reference face), or as the cross product of two
/// crossed edges, each turning with its body (`contact/turning.rs`). Fixed geometry keeps it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Follows {
    /// Neither: frozen for the step.
    #[default]
    Neither,
    /// Side A.
    A,
    /// Side B.
    B,
    /// Crossed edges: their directions on A and on B (world, when found).
    Edges(DVec3, DVec3),
    /// A ball against A's shape: measured to its closest point at every evaluation.
    ShapeA,
    /// A ball against B's shape.
    ShapeB,
}

/// A potential contact found at the start of a step: a point on A, a point on B, and the
/// normal from B to A. Its gap is n·(p_A − p_B) − r_A − r_B (negative when penetrating). For a
/// sphere the point is its centre and r its radius; the surface point is p − n·r_A on A and
/// p + n·r_B on B.
#[derive(Clone, Debug, PartialEq)]
pub struct RawContact {
    /// Point on A (world).
    pub point_a: DVec3,
    /// Point on B (world).
    pub point_b: DVec3,
    /// Unit normal from B to A.
    pub normal: DVec3,
    /// Radius of A at the contact (spheres; 0 for a point on a surface).
    pub radius_a: f64,
    /// Radius of B at the contact.
    pub radius_b: f64,
    /// What touches on A and on B (sets the curvature of a point contact).
    pub features: [Feature; 2],
    /// For a corner of a face pressed against a plane: which, and the part of the face that
    /// presses. The four corners are consecutive contacts (`contact/law.rs`).
    pub patch: Option<PatchVertex>,
    /// How B's surface curves at the contact (the ground's shape operator, `ground.rs`), if it
    /// is not flat.
    pub bend: Option<DMat3>,
    /// Which side's turning the normal follows.
    pub follows: Follows,
    /// Added to the radii: how far the point presses before it touches (a bump under a face
    /// that already presses, `ground.rs`).
    pub inset: f64,
}

/// A corner of a face pressed against a plane: one of four, in order around the face.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchVertex {
    /// Which corner (0..4).
    pub index: u8,
    /// Whether the corner is the contact's point on A (else on B; the other point is its
    /// projection on the plane).
    pub corner_on_a: bool,
    /// The part of the face that can press.
    pub domain: Domain,
    /// For a face on the ground: the ground's normal under the face, which it was found turned
    /// towards (`ground_rise::under_face`): its patch weighs its facing with it, so it enters as
    /// that weight rises from zero.
    pub facing: Option<DVec3>,
}

/// The part of a face that can press against a plane (the face clipped to the other face's
/// outline, or a hull's face in its rectangle): a convex polygon in the face's coordinates,
/// s from corner 0 towards corner 1 and t from corner 0 towards corner 3, both 0..1.
#[derive(Clone, Debug, PartialEq)]
pub enum Domain {
    /// The whole face.
    Whole,
    /// A polygon: its corners (s, t), in order (one list for the patch's four contacts).
    Part(Arc<[[f64; 2]]>),
    /// A polygon over curved ground, in triangles fine enough to follow it, each of their
    /// corners measured to the ground under it (`ground_rise.rs`).
    Sampled(Arc<Sampled>),
}

/// A face's domain over curved ground (`Domain::Sampled`): the face presses with the
/// ground's own depth at the corners of its triangles, each measured to the ground under it
/// to second order (as `Measure::Curved`), and linearly across each triangle.
#[derive(Clone, Debug, PartialEq)]
pub struct Sampled {
    /// The polygon's corners (s, t), in order.
    pub points: Vec<[f64; 2]>,
    /// The triangles' corners (s, t).
    pub vertices: Vec<[f64; 2]>,
    /// The ground under each: its surface point and outward normal there.
    pub feet: Vec<(DVec3, DVec3)>,
    /// The ground's shape operator under the face.
    pub bend: DMat3,
    /// The triangles: their corners, indices into `vertices`.
    pub triangles: Vec<[usize; 3]>,
}

/// The whole face's corners.
const SQUARE: [[f64; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

impl Domain {
    /// The whole face.
    pub const WHOLE: Self = Self::Whole;

    /// The polygon with corners `points` (s, t), in order.
    pub fn part(points: &[[f64; 2]]) -> Self {
        Self::Part(points.into())
    }

    /// The polygon's corners.
    pub fn points(&self) -> &[[f64; 2]] {
        match self {
            Self::Whole => &SQUARE,
            Self::Part(points) => points,
            Self::Sampled(sampled) => &sampled.points,
        }
    }

    /// Over curved ground: the triangles and the ground under their corners.
    pub fn sampled(&self) -> Option<&Arc<Sampled>> {
        match self {
            Self::Sampled(sampled) => Some(sampled),
            _ => None,
        }
    }
}

impl RawContact {
    /// The gap at detection time (m).
    pub fn gap(&self) -> f64 {
        self.normal.dot(self.point_a - self.point_b) - self.offset()
    }

    /// The radii subtracted from the distance between the points (m).
    pub fn offset(&self) -> f64 {
        self.radius_a + self.radius_b + self.inset
    }
}
