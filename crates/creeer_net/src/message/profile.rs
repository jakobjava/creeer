//! A server's profile, sent to a client that asked for it (F7.6): once a second, every
//! tick's length since the last report and the longest tick's spans (`creeer_core::profile`).

use super::{read_text, text};
use creeer_core::profile::{Frame, Span};
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};

/// What a report carries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileReport {
    /// Each tick's length since the last report (µs), oldest first.
    pub durations: Vec<u32>,
    /// The longest of them, its spans.
    pub worst: Frame,
}

impl ProfileReport {
    /// The report of `frames` (the ticks since the last one), if any.
    pub fn of(frames: &[Frame]) -> Option<Self> {
        let worst = frames.iter().max_by_key(|f| f.duration)?;
        Some(Self {
            durations: frames
                .iter()
                .map(|f| u32::try_from(f.duration / 1000).unwrap_or(u32::MAX))
                .collect(),
            worst: worst.clone(),
        })
    }
}

impl Encode for ProfileReport {
    fn encode(&self, w: &mut Writer) {
        w.varint(self.durations.len() as u64);
        for d in &self.durations {
            w.varint(u64::from(*d));
        }
        let f = &self.worst;
        w.varint(f.tick);
        w.varint(f.duration);
        for list in [&f.names, &f.threads] {
            w.varint(list.len() as u64);
            for s in list {
                text(w, s);
            }
        }
        w.varint(f.spans.len() as u64);
        for s in &f.spans {
            w.varint(u64::from(s.name));
            w.varint(u64::from(s.thread));
            w.u8(s.depth);
            w.varint(s.start);
            w.varint(s.end - s.start);
        }
    }
}

impl Decode for ProfileReport {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let durations = (0..r.count(1)?)
            .map(|_| u32::try_from(r.varint()?).map_err(|_| r.error("tick length")))
            .collect::<Result<_, _>>()?;
        let (tick, duration) = (r.varint()?, r.varint()?);
        let mut lists = [Vec::new(), Vec::new()];
        for list in &mut lists {
            for _ in 0..r.count(1)? {
                list.push(read_text(r)?);
            }
        }
        let [names, threads] = lists;
        let index = |r: &mut Reader<'_>, len: usize| {
            u16::try_from(r.varint()?)
                .ok()
                .filter(|i| usize::from(*i) < len)
                .ok_or_else(|| r.error("span index"))
        };
        let spans = (0..r.count(5)?)
            .map(|_| {
                let (name, thread) = (index(r, names.len())?, index(r, threads.len())?);
                let (depth, start) = (r.u8()?, r.varint()?);
                let end = start
                    .checked_add(r.varint()?)
                    .ok_or_else(|| r.error("span end"))?;
                Ok(Span {
                    name,
                    thread,
                    depth,
                    start,
                    end,
                })
            })
            .collect::<Result<_, DecodeError>>()?;
        Ok(Self {
            durations,
            worst: Frame {
                tick,
                duration,
                names,
                threads,
                spans,
            },
        })
    }
}
