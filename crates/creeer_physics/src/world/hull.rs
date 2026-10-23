//! Convex hulls: the collision shape of anything convex with flat faces (a rock, a wedge, a
//! cut stone, a casing).
//!
//! A hull is built from points (incremental construction: a tetrahedron of extreme points,
//! then each point outside the hull so far replaces the faces it sees with a fan from the
//! horizon). Triangles in one plane are merged into one face, a convex polygon, and corners
//! that lie on a straight edge are dropped: a face presses as one flat punch of its whole
//! area (`contact/patch.rs`), so a square face split in two triangles would be stiffer than
//! the square. The hull is centred on its centroid, where its body's node is, and its mass
//! properties are exact for a solid of uniform density.

use super::hull_build::{edges, merge, triangles};
use super::hull_narrow::rectangle;
use creeer_math::glam::{DMat3, DVec3};

/// A face of a hull.
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    /// Outward unit normal.
    pub normal: DVec3,
    /// n·x on the face (m).
    pub offset: f64,
    /// Its corners (indices into the hull's vertices), counter-clockwise seen from outside.
    pub ring: Vec<usize>,
    /// The rectangle of least area around it (`hull_narrow::rectangle`), in the hull's frame:
    /// its patch's corners. Found once, so a face whose rectangles tie keeps the same one
    /// however the hull turns. (Found again at every step in the world's frame, a rock's face
    /// cycled through three, its corners' friction anchors were lost each time, and it crept
    /// down a hump, F5.)
    pub rectangle: [DVec3; 4],
}

/// An edge of a hull: its two corners and the faces on either side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    /// The corners (indices into the hull's vertices).
    pub ends: [usize; 2],
    /// The face that runs from `ends[0]` to `ends[1]` around its ring, and the other.
    pub faces: [usize; 2],
}

/// A convex polyhedron centred on its centroid.
#[derive(Clone, Debug, PartialEq)]
pub struct Hull {
    vertices: Vec<DVec3>,
    faces: Vec<Face>,
    edges: Vec<Edge>,
    volume: f64,
    /// ∫ x·xᵀ dV / V about the centroid (m²).
    spread: DMat3,
    radius: f64,
}

/// Why points make no hull.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HullError {
    /// A point is not finite.
    NotFinite,
    /// The points lie in one plane (or on a line, or are one point): no volume.
    Flat,
}

/// Points this close to a plane, relative to the points' extent, lie in it.
const PLANAR: f64 = 1e-9;

impl Hull {
    /// The hull of `points`, centred on its centroid, and where that centroid was.
    ///
    /// # Errors
    /// If a point is not finite or the points enclose no volume.
    pub fn new(points: &[DVec3]) -> Result<(Self, DVec3), HullError> {
        if points.iter().any(|p| !p.is_finite()) {
            return Err(HullError::NotFinite);
        }
        let (lo, hi) = points.iter().fold(
            (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
            |(lo, hi), &p| (lo.min(p), hi.max(p)),
        );
        let eps = PLANAR * (hi - lo).length();
        let triangles = triangles(points, eps)?;
        let (vertices, faces) = merge(points, &triangles, eps);
        let mut hull = Self {
            edges: edges(&faces),
            vertices,
            faces,
            volume: 0.0,
            spread: DMat3::ZERO,
            radius: 0.0,
        };
        let centroid = hull.centre();
        Ok((hull, centroid))
    }

    /// A box of half extents `half`.
    pub fn cuboid(half: DVec3) -> Self {
        let corners: Vec<DVec3> = (0..8)
            .map(|c| {
                let s = |bit: usize| if c & bit == 0 { -1.0 } else { 1.0 };
                half * DVec3::new(s(1), s(2), s(4))
            })
            .collect();
        Self::new(&corners).expect("a box has volume").0
    }

    /// The same hull about `origin` (in its frame) instead of its centroid: for a body whose
    /// mass is not spread evenly through its shape, its node at its centre of mass (a felled
    /// tree, its crown's mass above its stem's hull). Its `inertia` stays about the centroid.
    pub fn about(mut self, origin: DVec3) -> Self {
        for v in &mut self.vertices {
            *v -= origin;
        }
        for face in &mut self.faces {
            face.offset -= face.normal.dot(origin);
            for c in &mut face.rectangle {
                *c -= origin;
            }
        }
        self.radius = self.vertices.iter().map(|v| v.length()).fold(0.0, f64::max);
        self
    }

    /// The corners (m, about the centroid).
    pub fn vertices(&self) -> &[DVec3] {
        &self.vertices
    }

    /// The faces.
    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    /// The edges.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// The volume (m³).
    pub fn volume(&self) -> f64 {
        self.volume
    }

    /// Radius of a ball about the centroid that contains the hull (m).
    pub fn bounding_radius(&self) -> f64 {
        self.radius
    }

    /// Rotational inertia (kg·m², about the centroid) of a solid hull of mass `mass`.
    pub fn inertia(&self, mass: f64) -> DMat3 {
        let s = self.spread;
        (DMat3::IDENTITY * (s.x_axis.x + s.y_axis.y + s.z_axis.z) - s) * mass
    }

    /// The corner furthest along `direction`.
    pub fn support(&self, direction: DVec3) -> usize {
        let mut best = (0, f64::NEG_INFINITY);
        for (i, v) in self.vertices.iter().enumerate() {
            let d = v.dot(direction);
            if d > best.1 {
                best = (i, d);
            }
        }
        best.0
    }

    /// The corners of face `f`, in order around it.
    pub fn ring(&self, f: usize) -> impl Iterator<Item = DVec3> + '_ {
        self.faces[f].ring.iter().map(|&i| self.vertices[i])
    }

    /// Works out the volume and the spread about the centroid (fans of tetrahedra from an
    /// inner point), moves the hull onto its centroid, and returns where that was.
    fn centre(&mut self) -> DVec3 {
        #[allow(clippy::cast_precision_loss)] // a few corners
        let inner = self.vertices.iter().sum::<DVec3>() / self.vertices.len() as f64;
        let (mut volume, mut moment, mut second) = (0.0, DVec3::ZERO, DMat3::ZERO);
        let outer = |v: DVec3| DMat3::from_cols(v * v.x, v * v.y, v * v.z);
        for face in &self.faces {
            let p = |k: usize| self.vertices[face.ring[k]] - inner;
            for k in 1..face.ring.len() - 1 {
                let (a, b, c) = (p(0), p(k), p(k + 1));
                let det = a.dot(b.cross(c));
                volume += det / 6.0;
                moment += (a + b + c) * (det / 24.0);
                // ∫ x·xᵀ over the tetrahedron (0, a, b, c).
                second += (outer(a) + outer(b) + outer(c) + outer(a + b + c)) * (det / 120.0);
            }
        }
        let offset = moment / volume;
        let centroid = inner + offset;
        self.volume = volume;
        self.spread = (second - outer(offset) * volume) * (1.0 / volume);
        for v in &mut self.vertices {
            *v -= centroid;
        }
        for face in &mut self.faces {
            face.offset -= face.normal.dot(centroid);
            let ring: Vec<DVec3> = face.ring.iter().map(|&v| self.vertices[v]).collect();
            face.rectangle = rectangle(&ring, face.normal);
        }
        self.radius = self.vertices.iter().map(|v| v.length()).fold(0.0, f64::max);
        centroid
    }
}

impl creeer_math::codec::Encode for Hull {
    fn encode(&self, w: &mut creeer_math::codec::Writer) {
        w.put(&self.vertices);
        w.varint(self.faces.len() as u64);
        for f in &self.faces {
            w.put(&f.normal);
            w.f64(f.offset);
            w.put(&f.ring);
            for c in f.rectangle {
                w.put(&c);
            }
        }
        w.varint(self.edges.len() as u64);
        for e in &self.edges {
            w.put(&e.ends[0]);
            w.put(&e.ends[1]);
            w.put(&e.faces[0]);
            w.put(&e.faces[1]);
        }
        w.f64(self.volume);
        w.put(&self.spread);
        w.f64(self.radius);
    }
}

impl creeer_math::codec::Decode for Hull {
    fn decode(
        r: &mut creeer_math::codec::Reader<'_>,
    ) -> Result<Self, creeer_math::codec::DecodeError> {
        let vertices: Vec<DVec3> = r.get()?;
        let faces = (0..r.count(8)?)
            .map(|_| {
                Ok(Face {
                    normal: r.get()?,
                    offset: r.finite()?,
                    ring: r.get()?,
                    rectangle: [r.get()?, r.get()?, r.get()?, r.get()?],
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let edges = (0..r.count(4)?)
            .map(|_| {
                Ok(Edge {
                    ends: [r.get()?, r.get()?],
                    faces: [r.get()?, r.get()?],
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (volume, spread, radius) = (r.finite()?, r.get()?, r.finite()?);
        let n = vertices.len();
        let corners_ok = faces
            .iter()
            .all(|f| f.ring.len() >= 3 && f.ring.iter().all(|&i| i < n));
        let edges_ok = edges
            .iter()
            .all(|e| e.ends.iter().all(|&i| i < n) && e.faces.iter().all(|&f| f < faces.len()));
        if n < 4 || faces.len() < 4 || !corners_ok || !edges_ok || volume <= 0.0 {
            return Err(r.error("not a hull"));
        }
        Ok(Self {
            vertices,
            faces,
            edges,
            volume,
            spread,
            radius,
        })
    }
}
