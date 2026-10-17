//! What a client is told about an entity once, when it comes into view: its shape, and whose
//! avatar it is. Its state then comes in snapshots (`snapshot.rs`).

use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};
use creeer_math::glam::DVec3;

/// An entity's id, the same for every client and stable for its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub u32);

/// The shape of a rigid body as a client draws it.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A box with these half extents (m).
    Box(DVec3),
    /// A ball of this radius (m).
    Sphere(f64),
    /// A convex hull of these corners (m, about its centroid).
    Hull(Vec<DVec3>),
}

/// A structure (members between nodes) as a client draws it: its nodes as they are, relative
/// to node 0 in node 0's frame, and its members as pairs of nodes, drawn as tubes of
/// `radius`. Its frame is node 0's state in snapshots; its shape changes only where it
/// deforms for good ([`crate::message::ServerMessage::Plastic`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Nodes relative to node 0, in its frame (m).
    pub nodes: Vec<DVec3>,
    /// Members: their two nodes.
    pub members: Vec<[u32; 2]>,
    /// The members' radius as drawn (m).
    pub radius: f64,
}

/// What an entity is.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A rigid body.
    Body(Shape),
    /// A player's avatar (a ball of this radius), and whose it is.
    Avatar {
        /// Its radius (m).
        radius: f64,
        /// The player's name.
        player: String,
    },
    /// A structure.
    Structure(Frame),
}

impl Encode for EntityId {
    fn encode(&self, w: &mut Writer) {
        w.varint(u64::from(self.0));
    }
}

impl Decode for EntityId {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        u32::try_from(r.varint()?)
            .map(Self)
            .map_err(|_| r.error("entity id beyond 32 bits"))
    }
}

impl Encode for Kind {
    fn encode(&self, w: &mut Writer) {
        match self {
            Self::Body(Shape::Box(half)) => {
                w.u8(0);
                w.put(half);
            }
            Self::Body(Shape::Sphere(radius)) => {
                w.u8(1);
                w.f64(*radius);
            }
            Self::Body(Shape::Hull(corners)) => {
                w.u8(2);
                w.put(corners);
            }
            Self::Avatar { radius, player } => {
                w.u8(3);
                w.f64(*radius);
                w.put(player);
            }
            Self::Structure(frame) => {
                w.u8(4);
                w.f64(frame.radius);
                w.put(&frame.nodes);
                w.varint(frame.members.len() as u64);
                for [a, b] in &frame.members {
                    w.varint(u64::from(*a));
                    w.varint(u64::from(*b));
                }
            }
        }
    }
}

impl Decode for Kind {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let node = |r: &mut Reader<'_>, count: usize| {
            u32::try_from(r.varint()?)
                .ok()
                .filter(|&n| (n as usize) < count)
                .ok_or_else(|| r.error("member's node beyond the nodes"))
        };
        Ok(match r.u8()? {
            0 => Self::Body(Shape::Box(r.get()?)),
            1 => Self::Body(Shape::Sphere(r.finite()?)),
            2 => Self::Body(Shape::Hull(r.get()?)),
            3 => Self::Avatar {
                radius: r.finite()?,
                player: r.get()?,
            },
            4 => {
                let radius = r.finite()?;
                let nodes: Vec<DVec3> = r.get()?;
                let count = r.count(2)?;
                let members = (0..count)
                    .map(|_| Ok([node(r, nodes.len())?, node(r, nodes.len())?]))
                    .collect::<Result<_, DecodeError>>()?;
                Self::Structure(Frame {
                    nodes,
                    members,
                    radius,
                })
            }
            _ => return Err(r.error("unknown kind of entity")),
        })
    }
}
