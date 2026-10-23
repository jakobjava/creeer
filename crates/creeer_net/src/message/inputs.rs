//! Avatars' inputs as they are written: each against the one before it.

use super::AvatarInput;
use crate::quant::Quantised;
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};

/// Numbered one past the input before it (its number is not written).
/// Steps of an input's walk per m/s (8 mm/s: finer than any hand on a stick).
pub const WALK_SCALE: f64 = 128.0;
/// Steps of an input's thrust per its most (1 %).
pub const THRUST_SCALE: f64 = 100.0;

pub(super) const JUMP: u8 = 1;
pub(super) const SAME_WALK: u8 = 2;
const NEXT: u8 = 4;
const THRUST: u8 = 8;
const STOPPING: u8 = 16;

/// Writes inputs, each against the one before it: its number only if it is not the next, and
/// its walk and thrust only if they changed (a byte for an input that goes on as before).
pub(super) fn write_inputs(w: &mut Writer, inputs: &[AvatarInput]) {
    w.varint(inputs.len() as u64);
    let (mut seq, mut walk, mut thrust) = (0u32, Quantised::default(), Quantised::default());
    for input in inputs {
        let now = Quantised::new(input.walk, WALK_SCALE);
        let push = Quantised::new(input.thrust, THRUST_SCALE);
        let next = seq != 0 && input.seq == seq.wrapping_add(1);
        let flags = if input.jump { JUMP } else { 0 }
            | if now == walk { SAME_WALK } else { 0 }
            | if next { NEXT } else { 0 }
            | if push == thrust { 0 } else { THRUST }
            | if input.stopping { STOPPING } else { 0 };
        w.u8(flags);
        if !next {
            w.varint(u64::from(input.seq.wrapping_sub(seq)));
        }
        if now != walk {
            now.write(w, walk);
        }
        if push != thrust {
            push.write(w, thrust);
        }
        (seq, walk, thrust) = (input.seq, now, push);
    }
}

pub(super) fn read_inputs(r: &mut Reader<'_>) -> Result<Vec<AvatarInput>, DecodeError> {
    let (mut seq, mut walk, mut thrust) = (0u32, Quantised::default(), Quantised::default());
    (0..r.count(1)?)
        .map(|_| {
            let flags = r.u8()?;
            if flags & !(JUMP | SAME_WALK | NEXT | THRUST | STOPPING) != 0 {
                return Err(r.error("unknown input flags"));
            }
            let step = if flags & NEXT != 0 {
                1
            } else {
                u32::try_from(r.varint()?).map_err(|_| r.error("input number"))?
            };
            seq = seq
                .checked_add(step)
                .filter(|_| step > 0)
                .ok_or_else(|| r.error("inputs out of order"))?;
            if flags & SAME_WALK == 0 {
                walk = Quantised::read(r, walk)?;
            }
            if flags & THRUST != 0 {
                thrust = Quantised::read(r, thrust)?;
            }
            Ok(AvatarInput {
                seq,
                walk: walk.value(WALK_SCALE),
                jump: flags & JUMP != 0,
                thrust: thrust.value(THRUST_SCALE),
                stopping: flags & STOPPING != 0,
            })
        })
        .collect()
}

impl Encode for AvatarInput {
    fn encode(&self, w: &mut Writer) {
        write_inputs(w, std::slice::from_ref(self));
    }
}

impl Decode for AvatarInput {
    fn decode(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        match read_inputs(r)?.as_slice() {
            [input] => Ok(*input),
            _ => Err(r.error("not one input")),
        }
    }
}
