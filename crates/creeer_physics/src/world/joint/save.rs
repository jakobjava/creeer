//! Joints as a save holds them (from the save format's version 7): everything a joint is,
//! and the multipliers it carries into the next step, so a loaded world goes on as the
//! saved one would have.

use super::{Drive, Joint, Kind, Multipliers};
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};

fn put_range(w: &mut Writer, range: Option<(f64, f64)>) {
    w.put(&range.map(|r| r.0));
    w.put(&range.map(|r| r.1));
}

fn get_range(r: &mut Reader<'_>) -> Result<Option<(f64, f64)>, DecodeError> {
    match (r.get::<Option<f64>>()?, r.get::<Option<f64>>()?) {
        (Some(lo), Some(hi)) if lo <= hi => Ok(Some((lo, hi))),
        (None, None) => Ok(None),
        _ => Err(r.error("a joint's range")),
    }
}

impl Encode for Joint {
    fn encode(&self, w: &mut Writer) {
        w.varint(self.a.0 as u64);
        w.varint(self.a.1 as u64);
        w.put(&self.b.map(|b| b.0));
        w.put(&self.b.map(|b| b.1));
        w.put(&self.at_a);
        w.put(&self.at_b);
        w.put(&self.frame_a);
        w.put(&self.frame_b);
        match self.kind {
            Kind::Ball { cone, twist } => {
                w.u8(0);
                w.put(&cone);
                put_range(w, twist);
            }
            Kind::Hinge { range } => {
                w.u8(1);
                put_range(w, range);
            }
        }
        w.f64(self.damping);
        w.f64(self.friction);
        w.u8(u8::from(self.drive.is_some()));
        if let Some(d) = self.drive {
            w.put(&d.target);
            w.f64(d.stiffness);
            w.f64(d.strength);
        }
        w.u8(u8::from(self.collide));
        w.put(&self.anchor);
        let m = self.multipliers;
        w.put(&m.point);
        w.put(&m.axis);
        for l in m.limits {
            w.f64(l);
        }
    }
}

fn index(r: &mut Reader<'_>) -> Result<usize, DecodeError> {
    usize::try_from(r.varint()?).map_err(|_| r.error("an index"))
}

impl Decode for Joint {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let a = (index(r)?, index(r)?);
        let b = match (r.get::<Option<usize>>()?, r.get::<Option<usize>>()?) {
            (Some(body), Some(node)) => Some((body, node)),
            (None, None) => None,
            _ => return Err(r.error("half a joint's side")),
        };
        let (at_a, at_b, frame_a, frame_b) = (r.get()?, r.get()?, r.get()?, r.get()?);
        let kind = match r.u8()? {
            0 => Kind::Ball {
                cone: r.get()?,
                twist: get_range(r)?,
            },
            1 => Kind::Hinge {
                range: get_range(r)?,
            },
            _ => return Err(r.error("an unknown joint")),
        };
        let (damping, friction) = (r.finite()?, r.finite()?);
        let drive = match r.u8()? {
            0 => None,
            1 => Some(Drive {
                target: r.get()?,
                stiffness: r.finite()?,
                strength: r.finite()?,
            }),
            _ => return Err(r.error("not a flag")),
        };
        let collide = match r.u8()? {
            0 => false,
            1 => true,
            _ => return Err(r.error("not a flag")),
        };
        let anchor: Option<creeer_math::glam::DQuat> = r.get()?;
        let multipliers = Multipliers {
            point: r.get()?,
            axis: r.get()?,
            limits: [r.finite()?, r.finite()?, r.finite()?],
        };
        if damping < 0.0 || friction < 0.0 || drive.is_some_and(|d| d.stiffness < 0.0) {
            return Err(r.error("a joint's damping, friction or drive below nought"));
        }
        Ok(Self {
            a,
            b,
            at_a,
            at_b,
            frame_a,
            frame_b,
            kind,
            damping,
            friction,
            drive,
            collide,
            anchor,
            multipliers,
        })
    }
}
