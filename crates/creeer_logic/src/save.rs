//! A network as a save holds it: its blocks, their wiring and state, every output's value, and
//! its scripts by their source, limits, `memory` and random generator (`script.rs`: a script
//! starts again from its source on loading; one that had stopped stays stopped).

use crate::block::{Compare, Kind, Pid, State};
use crate::network::{Block, BlockId, Network};
use crate::script::{Limits, Script};
use crate::signal::Signal;
use creeer_math::codec::{DecodeError, Reader, Writer};

const COMPARES: [Compare; 6] = [
    Compare::Greater,
    Compare::Less,
    Compare::AtLeast,
    Compare::AtMost,
    Compare::Equal,
    Compare::NotEqual,
];

fn write_kind(kind: Kind, w: &mut Writer) {
    let simple = |w: &mut Writer, tag: u8| w.u8(tag);
    match kind {
        Kind::Constant(s) => {
            w.u8(0);
            w.put(&s);
        }
        Kind::Sensor => simple(w, 1),
        Kind::Actuator => simple(w, 2),
        Kind::Not => simple(w, 3),
        Kind::And => simple(w, 4),
        Kind::Or => simple(w, 5),
        Kind::Xor => simple(w, 6),
        Kind::Nand => simple(w, 7),
        Kind::Nor => simple(w, 8),
        Kind::SrLatch => simple(w, 9),
        Kind::DFlipFlop => simple(w, 10),
        Kind::OnDelay(t) | Kind::OffDelay(t) | Kind::Pulse(t) => {
            w.u8(match kind {
                Kind::OnDelay(_) => 11,
                Kind::OffDelay(_) => 12,
                _ => 13,
            });
            w.f64(t);
        }
        Kind::Compare(op) => {
            w.u8(14);
            w.u8(u8::try_from(COMPARES.iter().position(|&c| c == op).unwrap_or(0)).unwrap_or(0));
        }
        Kind::Schmitt { low, high } => {
            w.u8(15);
            w.f64(low);
            w.f64(high);
        }
        Kind::Add => simple(w, 16),
        Kind::Sub => simple(w, 17),
        Kind::Mul => simple(w, 18),
        Kind::Div => simple(w, 19),
        Kind::Min => simple(w, 20),
        Kind::Max => simple(w, 21),
        Kind::Select => simple(w, 22),
        Kind::Pid(p) => {
            w.u8(23);
            for x in [p.kp, p.ki, p.kd, p.min, p.max] {
                w.f64(x);
            }
        }
        Kind::Script(i) => {
            w.u8(24);
            w.put(&i);
        }
    }
}

fn read_kind(r: &mut Reader<'_>) -> Result<Kind, DecodeError> {
    Ok(match r.u8()? {
        0 => Kind::Constant(r.get()?),
        1 => Kind::Sensor,
        2 => Kind::Actuator,
        3 => Kind::Not,
        4 => Kind::And,
        5 => Kind::Or,
        6 => Kind::Xor,
        7 => Kind::Nand,
        8 => Kind::Nor,
        9 => Kind::SrLatch,
        10 => Kind::DFlipFlop,
        11 => Kind::OnDelay(r.f64()?),
        12 => Kind::OffDelay(r.f64()?),
        13 => Kind::Pulse(r.f64()?),
        14 => Kind::Compare(
            *COMPARES
                .get(usize::from(r.u8()?))
                .ok_or_else(|| r.error("not a comparison"))?,
        ),
        15 => Kind::Schmitt {
            low: r.f64()?,
            high: r.f64()?,
        },
        16 => Kind::Add,
        17 => Kind::Sub,
        18 => Kind::Mul,
        19 => Kind::Div,
        20 => Kind::Min,
        21 => Kind::Max,
        22 => Kind::Select,
        23 => Kind::Pid(Pid {
            kp: r.f64()?,
            ki: r.f64()?,
            kd: r.f64()?,
            min: r.f64()?,
            max: r.f64()?,
        }),
        24 => Kind::Script(r.get()?),
        _ => return Err(r.error("not a block")),
    })
}

impl Network {
    /// The network as bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.varint(self.blocks.len() as u64);
        for (i, b) in self.blocks.iter().enumerate() {
            let state = self.state_of(i);
            write_kind(b.kind, &mut w);
            w.varint(b.inputs.len() as u64);
            for pin in &b.inputs {
                w.put(&pin.map(|(s, _)| s.0));
                w.put(&pin.map(|(_, k)| k));
            }
            w.put(&state.held);
            w.put(&state.last);
            w.f64(state.time);
            w.f64(state.previous);
        }
        let values: Vec<Signal> = (0..self.values.len()).map(|i| self.value_of(i)).collect();
        w.put(&values);
        w.varint(self.script_count() as u64);
        for s in (0..self.script_count()).map(|i| self.script_of(i)) {
            let (memory, rng) = s.saved();
            let (inputs, outputs) = s.pins();
            w.put(&s.source().to_owned());
            w.put(&inputs);
            w.put(&outputs);
            w.u64(s.limits().instructions);
            w.put(&s.limits().memory);
            w.bytes_with_length(&memory);
            for x in rng {
                w.u64(x);
            }
            w.put(&s.halted().map(str::to_owned));
        }
        w.finish()
    }

    /// A network from bytes written by [`Self::to_bytes`].
    ///
    /// # Errors
    /// If the bytes are not a whole network.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        let mut net = Network::new();
        for _ in 0..r.count(4)? {
            let kind = read_kind(&mut r)?;
            let inputs = (0..r.count(2)?)
                .map(|_| {
                    let (s, k): (Option<usize>, Option<usize>) = (r.get()?, r.get()?);
                    Ok(s.zip(k).map(|(s, k)| (BlockId(s), k)))
                })
                .collect::<Result<Vec<_>, DecodeError>>()?;
            let state = State {
                held: r.get()?,
                last: r.get()?,
                time: r.f64()?,
                previous: r.f64()?,
            };
            net.blocks.push(Block {
                kind,
                inputs,
                state,
                // (Set below, from the blocks' kinds.)
                first: 0,
            });
        }
        net.values = r.get::<Vec<Signal>>()?;
        for _ in 0..r.count(8)? {
            let source: String = r.get()?;
            let pins = (r.get()?, r.get()?);
            let limits = Limits {
                instructions: r.u64()?,
                memory: r.get()?,
            };
            let memory = r.bytes_with_length()?.to_vec();
            let rng = [r.u64()?, r.u64()?, r.u64()?, r.u64()?];
            let halted: Option<String> = r.get()?;
            net.scripts
                .push(Script::loaded(&source, pins, limits, rng, &memory, halted));
        }
        net.renumber_outputs()
            .ok_or_else(|| r.error("wiring to a missing block"))?;
        r.end()?;
        Ok(net)
    }
}
