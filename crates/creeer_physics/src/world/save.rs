//! The physics world as a save holds it: its settings, fixed geometry, every body (its
//! structure, solver history, shapes, surface and sleep), and what the next step continues
//! from: the friction anchors, and the energy the contacts held (from version 5). The ground
//! is not in it: it is the terrain's, which a save holds as its edits (`creeer_sim::save`);
//! whoever loads a world gives it its ground again.
//!
//! What a step finds again from the state (its contacts and groups) is left out, and what a
//! solver caches (its factorised matrix) is rebuilt; so a loaded world goes on as the saved
//! one would have, within the solver's tolerance, and saving it again gives the same bytes.

use super::anchor::{Anchor, Kept};
use super::left::Left;
use super::shape::{Shape, StaticShape};
use super::sleep::Sleep;
use super::surface::Surface;
use super::{Body, World};
use crate::structure::{Structure, read_solver, write_solver};
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};
use std::sync::Arc;

impl Encode for Shape {
    fn encode(&self, w: &mut Writer) {
        match self {
            Self::Box { half } => {
                w.u8(0);
                w.put(half);
            }
            Self::Sphere { radius } => {
                w.u8(1);
                w.f64(*radius);
            }
            Self::Hull(hull) => {
                w.u8(2);
                w.put(hull.as_ref());
            }
        }
    }
}

impl Decode for Shape {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let shape = match r.u8()? {
            0 => Self::Box { half: r.get()? },
            1 => Self::Sphere {
                radius: r.finite()?,
            },
            2 => Self::Hull(Arc::new(r.get()?)),
            _ => return Err(r.error("unknown shape")),
        };
        match &shape {
            Self::Box { half } if half.min_element() <= 0.0 => Err(r.error("a box without volume")),
            Self::Sphere { radius } if *radius <= 0.0 => Err(r.error("a ball without volume")),
            _ => Ok(shape),
        }
    }
}

impl Encode for StaticShape {
    fn encode(&self, w: &mut Writer) {
        match self {
            Self::Plane { normal, offset } => {
                w.u8(0);
                w.put(normal);
                w.f64(*offset);
            }
            Self::Box {
                center,
                rotation,
                half,
            } => {
                w.u8(1);
                w.put(center);
                w.put(rotation);
                w.put(half);
            }
        }
    }
}

impl Decode for StaticShape {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            0 => Self::Plane {
                normal: r.get()?,
                offset: r.finite()?,
            },
            1 => Self::Box {
                center: r.get()?,
                rotation: r.get()?,
                half: r.get()?,
            },
            _ => return Err(r.error("unknown fixed shape")),
        })
    }
}

impl Encode for Surface {
    fn encode(&self, w: &mut Writer) {
        for v in [self.youngs_modulus, self.poisson_ratio, self.rounding] {
            w.f64(v);
        }
    }
}

impl Decode for Surface {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            youngs_modulus: r.finite()?,
            poisson_ratio: r.finite()?,
            rounding: r.finite()?,
        })
    }
}

impl Encode for Body {
    fn encode(&self, w: &mut Writer) {
        w.put(&self.structure);
        write_solver(&self.solver, w);
        w.varint(self.shapes.len() as u64);
        for (node, shape) in &self.shapes {
            w.put(node);
            w.put(shape);
        }
        w.f64(self.friction);
        w.f64(self.restitution);
        w.put(&self.surface);
        w.put(&self.sleep.asleep);
        w.put(&self.sleep.still);
        w.put(&self.sleep.load);
        w.put(&self.crushing);
    }
}

impl Decode for Body {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let structure: Structure = r.get()?;
        let solver = read_solver(&structure, r)?;
        let shapes = (0..r.count(2)?)
            .map(|_| Ok((r.get()?, r.get()?)))
            .collect::<Result<Vec<(usize, Shape)>, DecodeError>>()?;
        if shapes
            .iter()
            .any(|(node, _)| *node >= structure.node_count())
        {
            return Err(r.error("a shape on a node the body lacks"));
        }
        let (friction, restitution, surface) = (r.finite()?, r.finite()?, r.get()?);
        let sleep = Sleep {
            asleep: r.get()?,
            still: r.get()?,
            load: r.get()?,
        };
        Ok(Self {
            structure,
            solver,
            shapes,
            friction,
            restitution,
            surface,
            sleep,
            crushing: r.get()?,
        })
    }
}

fn write_anchor(a: &Anchor, w: &mut Writer) {
    w.put(&a.a.0);
    w.put(&a.a.1);
    w.put(&a.local_a);
    w.put(&a.b.map(|b| b.0));
    w.put(&a.b.map(|b| b.1));
    match a.kept {
        Kept::Point(p) => {
            w.u8(0);
            w.put(&p);
        }
        Kept::Slip(s) => {
            w.u8(1);
            w.put(&s);
        }
    }
}

fn read_anchor(r: &mut Reader<'_>) -> Result<Anchor, DecodeError> {
    let a = (r.get()?, r.get()?);
    let local_a = r.get()?;
    let b = match (r.get::<Option<usize>>()?, r.get::<Option<usize>>()?) {
        (Some(body), Some(node)) => Some((body, node)),
        (None, None) => None,
        _ => return Err(r.error("half an anchor")),
    };
    let kept = match r.u8()? {
        0 => Kept::Point(r.get()?),
        1 => Kept::Slip(r.get()?),
        _ => return Err(r.error("unknown anchor")),
    };
    Ok(Anchor {
        a,
        local_a,
        b,
        kept,
    })
}

impl World {
    /// The world as bytes (`save.rs`; its ground left out).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.f64(self.dt);
        w.f64(self.static_friction);
        w.f64(self.static_restitution);
        w.put(&self.static_surface);
        w.put(&self.planet);
        w.put(&self.statics);
        w.put(&self.bodies);
        w.varint(self.anchors.len() as u64);
        for a in &self.anchors {
            write_anchor(a, &mut w);
        }
        // (Version 5 on.)
        w.varint(self.left.energies.len() as u64);
        for &((a, b), energy) in &self.left.energies {
            for side in [a, b] {
                w.put(&side.map(|s| s.0));
                w.put(&side.map(|s| s.1));
            }
            w.f64(energy);
        }
        w.put(&self.left.stepped);
        w.finish()
    }

    /// A world from bytes written by [`Self::to_bytes`] of this version, without ground.
    ///
    /// # Errors
    /// If the bytes are not a whole world.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        Self::from_bytes_of(bytes, u32::MAX)
    }

    /// A world from bytes written by the save format's version `version`, without ground.
    ///
    /// # Errors
    /// If the bytes are not a whole world.
    pub fn from_bytes_of(bytes: &[u8], version: u32) -> Result<Self, DecodeError> {
        let mut r = Reader::versioned(bytes, version);
        let dt = r.finite()?;
        if dt <= 0.0 {
            return Err(r.error("a world without a time step"));
        }
        let mut world = Self::new(dt);
        world.static_friction = r.finite()?;
        world.static_restitution = r.finite()?;
        world.static_surface = r.get()?;
        world.planet = r.get()?;
        world.statics = r.get()?;
        world.bodies = r.get()?;
        let anchors = (0..r.count(8)?)
            .map(|_| read_anchor(&mut r))
            .collect::<Result<Vec<_>, _>>()?;
        let n = world.bodies.len();
        let fits = |(body, node): (usize, usize)| {
            body < n && node < world.bodies[body].structure.node_count()
        };
        if anchors
            .iter()
            .any(|a| !fits(a.a) || a.b.is_some_and(|b| !fits(b)))
        {
            return Err(r.error("an anchor on a body the world lacks"));
        }
        world.anchors = anchors;
        if r.version() >= 5 {
            world.left = read_left(&mut r, &world)?;
        }
        r.end()?;
        Ok(world)
    }
}

#[cfg(test)]
mod tests;

/// What the last step left in its contacts (`left.rs`), its bodies checked against `world`'s.
fn read_left(r: &mut Reader<'_>, world: &World) -> Result<Left, DecodeError> {
    let side = |r: &mut Reader<'_>| match (r.get::<Option<usize>>()?, r.get::<Option<usize>>()?) {
        (Some(body), Some(node)) => Ok(Some((body, node))),
        (None, None) => Ok(None),
        _ => Err(r.error("half a side")),
    };
    let energies = (0..r.count(8)?)
        .map(|_| Ok(((side(r)?, side(r)?), r.finite()?)))
        .collect::<Result<Vec<_>, DecodeError>>()?;
    let stepped: Vec<usize> = r.get()?;
    let n = world.bodies.len();
    let fits = |s: Option<(usize, usize)>| {
        s.is_none_or(|(body, node)| body < n && node < world.bodies[body].structure.node_count())
    };
    if !energies.iter().all(|((a, b), _)| fits(*a) && fits(*b)) || stepped.iter().any(|&b| b >= n) {
        return Err(r.error("energy left on a body the world lacks"));
    }
    Ok(Left { energies, stepped })
}
