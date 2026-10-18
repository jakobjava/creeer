//! The `MACH` section (version 7, F7.4): every machine of the world, as its abstract self
//! keeps it (`crate::abstraction::snapshot`): its kind, where it stands, the tick it was
//! built on, its mode, time and stocks, its stretch under way, its fast states.

use super::SaveError;
use crate::abstraction::snapshot::{Snapshot, StretchNumbers};
use creeer_math::codec::{DecodeError, Reader, Writer};
use creeer_math::glam::DVec3;

/// A machine of the world.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedMachine {
    pub id: u32,
    /// What it is (`"powerhouse"`).
    pub kind: String,
    pub at: DVec3,
    pub born: u64,
    pub snapshot: Snapshot,
}

fn floats(w: &mut Writer, v: &[f64]) {
    w.varint(v.len() as u64);
    v.iter().for_each(|&x| w.f64(x));
}

fn read_floats(r: &mut Reader<'_>) -> Result<Vec<f64>, DecodeError> {
    (0..r.count(8)?).map(|_| r.f64()).collect()
}

/// The machines as the section holds them.
pub(super) fn machine_bytes(machines: &[SavedMachine]) -> Vec<u8> {
    let mut w = Writer::new();
    w.varint(machines.len() as u64);
    for m in machines {
        let s = &m.snapshot;
        w.u32(m.id);
        w.put(&m.kind);
        w.put(&m.at);
        w.u64(m.born);
        w.u8(u8::from(s.abstract_mode));
        w.f64(s.time);
        floats(&mut w, &s.amounts);
        floats(&mut w, &s.scale);
        w.bytes_with_length(&s.fast);
        match &s.stretch {
            None => w.u8(0),
            Some(st) => {
                w.u8(1);
                floats(&mut w, &st.rates);
                w.varint(st.trends.len() as u64);
                for &(kind, a, b) in &st.trends {
                    w.u8(kind);
                    w.f64(a);
                    w.f64(b);
                }
                [st.at, st.began, st.ends].iter().for_each(|&x| w.f64(x));
            }
        }
        match &s.measured {
            None => w.u8(0),
            Some((t, rates, stocks)) => {
                w.u8(1);
                w.f64(*t);
                floats(&mut w, rates);
                floats(&mut w, stocks);
            }
        }
    }
    w.finish()
}

/// The machines from the section's bytes.
pub(super) fn machines_from(bytes: &[u8]) -> Result<Vec<SavedMachine>, SaveError> {
    let mut r = Reader::new(bytes);
    let mut machines = Vec::new();
    for _ in 0..r.count(20)? {
        let (id, kind, at, born) = (r.u32()?, r.get::<String>()?, r.get::<DVec3>()?, r.u64()?);
        let abstract_mode = r.u8()? == 1;
        let time = r.f64()?;
        let (amounts, scale) = (read_floats(&mut r)?, read_floats(&mut r)?);
        let fast = r.bytes_with_length()?.to_vec();
        let stretch = match r.u8()? {
            0 => None,
            _ => {
                let rates = read_floats(&mut r)?;
                let trends = (0..r.count(17)?)
                    .map(|_| Ok((r.u8()?, r.f64()?, r.f64()?)))
                    .collect::<Result<Vec<_>, DecodeError>>()?;
                let (at, began, ends) = (r.f64()?, r.f64()?, r.f64()?);
                Some(StretchNumbers {
                    rates,
                    trends,
                    at,
                    began,
                    ends,
                })
            }
        };
        let measured = match r.u8()? {
            0 => None,
            _ => Some((r.f64()?, read_floats(&mut r)?, read_floats(&mut r)?)),
        };
        machines.push(SavedMachine {
            id,
            kind,
            at,
            born,
            snapshot: Snapshot {
                abstract_mode,
                time,
                amounts,
                stretch,
                measured,
                scale,
                fast,
            },
        });
    }
    r.end()?;
    Ok(machines)
}
