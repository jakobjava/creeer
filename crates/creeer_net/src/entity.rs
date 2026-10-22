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

/// How a player's character looks (G1.3e, ADR-0026 decisions 7 and 8): the player's colour,
/// skin tone and hair colour (indices into the palette's lists: `creeer_art::Palette`'s
/// `players`, `skin`, `hair`), and what it wears (wearables by name, `data/wearables.ron`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Look {
    pub colour: u8,
    pub skin: u8,
    pub hair: u8,
    pub outfit: Vec<String>,
}

impl Encode for Look {
    fn encode(&self, w: &mut Writer) {
        w.u8(self.colour);
        w.u8(self.skin);
        w.u8(self.hair);
        w.put(&self.outfit);
    }
}

impl Decode for Look {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            colour: r.u8()?,
            skin: r.u8()?,
            hair: r.u8()?,
            outfit: r.get()?,
        })
    }
}

/// What an entity is.
/// What a rigid body is made of, for a client to predict it (G1.2e, ADR-0025): what the
/// physics needs beyond its shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matter {
    /// Its mass (kg; its inertia follows from it and its shape, uniformly dense).
    pub mass: f64,
    /// Its surface's Coulomb friction and restitution.
    pub friction: f64,
    pub restitution: f64,
    /// Its surface: Young's modulus (Pa), Poisson's ratio, its edges' rounding (m).
    pub surface: [f64; 3],
    /// Whether a client may predict it: a lone rigid body, no joint's side.
    pub predicted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// A rigid body, and what it is made of.
    Body(Shape, Matter),
    /// A player's character (its shape is the game's: `creeer_sim::character`), whose, and how
    /// it looks. Fallen, it is its ragdoll's pelvis, the other segments [`Kind::Segment`]s.
    Character {
        /// The player's name.
        player: String,
        look: Look,
        fallen: bool,
        /// The tool in its right hand (G1.4c: an index into the tools' table), if any.
        wields: Option<u8>,
    },
    /// A segment of a fallen character's ragdoll (G1.3e): whose (the avatar, its pelvis) and
    /// which (`creeer_sim::character::bones::SEGMENT_NAMES`, 1 to 14), and its shape.
    Segment {
        person: EntityId,
        index: u8,
        shape: Shape,
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
            Self::Body(shape, matter) => {
                match shape {
                    Shape::Box(half) => {
                        w.u8(0);
                        w.put(half);
                    }
                    Shape::Sphere(radius) => {
                        w.u8(1);
                        w.f64(*radius);
                    }
                    Shape::Hull(corners) => {
                        w.u8(2);
                        w.put(corners);
                    }
                }
                for v in [matter.mass, matter.friction, matter.restitution] {
                    w.f64(v);
                }
                for v in matter.surface {
                    w.f64(v);
                }
                w.u8(u8::from(matter.predicted));
            }
            Self::Character {
                player,
                look,
                fallen,
                wields,
            } => {
                w.u8(3);
                w.put(player);
                w.put(look);
                w.u8(u8::from(*fallen));
                w.u8(wields.unwrap_or(crate::message::NONE));
            }
            Self::Segment {
                person,
                index,
                shape,
            } => {
                w.u8(5);
                w.put(person);
                w.u8(*index);
                match shape {
                    Shape::Box(half) => {
                        w.u8(0);
                        w.put(half);
                    }
                    Shape::Sphere(radius) => {
                        w.u8(1);
                        w.f64(*radius);
                    }
                    Shape::Hull(corners) => {
                        w.u8(2);
                        w.put(corners);
                    }
                }
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
            0 => Self::Body(Shape::Box(r.get()?), matter(r)?),
            1 => Self::Body(Shape::Sphere(r.finite()?), matter(r)?),
            2 => Self::Body(Shape::Hull(r.get()?), matter(r)?),
            3 => Self::Character {
                player: r.get()?,
                look: r.get()?,
                fallen: flag(r)?,
                wields: Some(r.u8()?).filter(|&t| t != crate::message::NONE),
            },
            5 => Self::Segment {
                person: r.get()?,
                index: r.u8()?,
                shape: match r.u8()? {
                    0 => Shape::Box(r.get()?),
                    1 => Shape::Sphere(r.finite()?),
                    2 => Shape::Hull(r.get()?),
                    _ => return Err(r.error("unknown shape")),
                },
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

fn flag(r: &mut Reader<'_>) -> Result<bool, DecodeError> {
    match r.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(r.error("not a flag")),
    }
}

fn matter(r: &mut Reader<'_>) -> Result<Matter, DecodeError> {
    let (mass, friction, restitution) = (r.finite()?, r.finite()?, r.finite()?);
    let surface = [r.finite()?, r.finite()?, r.finite()?];
    let predicted = flag(r)?;
    if mass <= 0.0 || friction < 0.0 || !(0.0..=1.0).contains(&restitution) {
        return Err(r.error("a body's matter out of range"));
    }
    Ok(Matter {
        mass,
        friction,
        restitution,
        surface,
        predicted,
    })
}
