//! Collision shapes.

use creeer_math::glam::{DMat3, DQuat, DVec3};

/// A shape attached to a node of a body, in the node's frame (centred on the node).
#[derive(Clone, Copy, Debug, PartialEq)]
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
}

impl Shape {
    /// Radius of a ball around the node that contains the shape.
    pub fn bounding_radius(&self) -> f64 {
        match self {
            Self::Sphere { radius } => *radius,
            Self::Box { half } => half.length(),
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

/// A potential contact found at the start of a step: a point on A, a point on B, and the
/// normal from B to A. Its gap is n·(p_A − p_B) − offset (negative when penetrating).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawContact {
    /// Point on A (world).
    pub point_a: DVec3,
    /// Point on B (world).
    pub point_b: DVec3,
    /// Unit normal from B to A.
    pub normal: DVec3,
    /// Radii to subtract from the distance between the points (spheres).
    pub offset: f64,
}

impl RawContact {
    /// The gap at detection time (m).
    pub fn gap(&self) -> f64 {
        self.normal.dot(self.point_a - self.point_b) - self.offset
    }
}
