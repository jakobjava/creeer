//! Avatars' inputs as they are written: each against the one before it.

use super::{AvatarInput, Hold};
use crate::quant::Quantised;
use creeer_math::codec::{Decode, DecodeError, Encode, Reader, Writer};
use creeer_math::det;
use creeer_math::glam::{DQuat, DVec3};

/// Numbered one past the input before it (its number is not written).
/// Steps of an input's walk per m/s (8 mm/s: finer than any hand on a stick).
pub const WALK_SCALE: f64 = 128.0;
/// Steps of an input's thrust per its most (1 %).
pub const THRUST_SCALE: f64 = 100.0;
/// Steps of a hold's offset per metre (about a millimetre), and of its turn per radian (a
/// quarter of a milliradian: 0.2 mm at a metre's reach).
pub const OFFSET_SCALE: f64 = 1024.0;
pub const TURN_SCALE: f64 = 4096.0;

pub(super) const JUMP: u8 = 1;
pub(super) const SAME_WALK: u8 = 2;
const NEXT: u8 = 4;
const THRUST: u8 = 8;
const STOPPING: u8 = 16;
/// Something held and steered (G2.10); unchanged from the input before; held steady.
const HOLDING: u8 = 32;
const SAME_HOLD: u8 = 64;
const STEADY: u8 = 128;

/// A turn as its rotation vector (axis times angle, the angle within π).
fn vector_of(q: DQuat) -> DVec3 {
    let q = if q.w < 0.0 { -q } else { q };
    let v = DVec3::new(q.x, q.y, q.z);
    let s = v.length();
    if s < 1e-12 {
        return v * 2.0;
    }
    v * (2.0 * det::atan2(s, q.w) / s)
}

/// The turn of rotation vector `v`.
fn turn_of(v: DVec3) -> DQuat {
    let angle = v.length();
    if angle < 1e-12 {
        return DQuat::from_xyzw(0.5 * v.x, 0.5 * v.y, 0.5 * v.z, 1.0).normalize();
    }
    let (s, c) = det::sin_cos(0.5 * angle);
    let axis = v / angle * s;
    DQuat::from_xyzw(axis.x, axis.y, axis.z, c)
}

/// A hold as the server reads it: its offset and turn quantised.
pub(crate) fn as_read(hold: Option<Hold>) -> Option<Hold> {
    quantised(hold).map(|(offset, turn, steady)| Hold {
        offset: offset.value(OFFSET_SCALE),
        turn: turn_of(turn.value(TURN_SCALE)),
        steady,
    })
}

/// A hold as written: its offset and turn quantised, and whether steady.
fn quantised(hold: Option<Hold>) -> Option<(Quantised, Quantised, bool)> {
    hold.map(|h| {
        (
            Quantised::new(h.offset, OFFSET_SCALE),
            Quantised::new(vector_of(h.turn), TURN_SCALE),
            h.steady,
        )
    })
}

/// Writes inputs, each against the one before it: its number only if it is not the next, and
/// its walk, thrust and hold only if they changed (a byte for an input that goes on as before).
pub(super) fn write_inputs(w: &mut Writer, inputs: &[AvatarInput]) {
    w.varint(inputs.len() as u64);
    let (mut seq, mut walk, mut thrust) = (0u32, Quantised::default(), Quantised::default());
    let mut held = (Quantised::default(), Quantised::default());
    for input in inputs {
        let now = Quantised::new(input.walk, WALK_SCALE);
        let push = Quantised::new(input.thrust, THRUST_SCALE);
        let hold = quantised(input.hold);
        let next = seq != 0 && input.seq == seq.wrapping_add(1);
        let same_hold = hold.is_some_and(|(o, t, _)| (o, t) == held);
        let flags = if input.jump { JUMP } else { 0 }
            | if now == walk { SAME_WALK } else { 0 }
            | if next { NEXT } else { 0 }
            | if push == thrust { 0 } else { THRUST }
            | if input.stopping { STOPPING } else { 0 }
            | if hold.is_some() { HOLDING } else { 0 }
            | if same_hold { SAME_HOLD } else { 0 }
            | if hold.is_some_and(|h| h.2) { STEADY } else { 0 };
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
        if let Some((offset, turn, _)) = hold
            && !same_hold
        {
            offset.write(w, held.0);
            turn.write(w, held.1);
            held = (offset, turn);
        }
        (seq, walk, thrust) = (input.seq, now, push);
    }
}

pub(super) fn read_inputs(r: &mut Reader<'_>) -> Result<Vec<AvatarInput>, DecodeError> {
    let (mut seq, mut walk, mut thrust) = (0u32, Quantised::default(), Quantised::default());
    let mut held = (Quantised::default(), Quantised::default());
    (0..r.count(1)?)
        .map(|_| {
            let flags = r.u8()?;
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
            let holding = flags & HOLDING != 0;
            if flags & (SAME_HOLD | STEADY) != 0 && !holding {
                return Err(r.error("a hold's flags without a hold"));
            }
            if holding && flags & SAME_HOLD == 0 {
                held = (Quantised::read(r, held.0)?, Quantised::read(r, held.1)?);
            }
            let hold = holding.then(|| Hold {
                offset: held.0.value(OFFSET_SCALE),
                turn: turn_of(held.1.value(TURN_SCALE)),
                steady: flags & STEADY != 0,
            });
            Ok(AvatarInput {
                seq,
                walk: walk.value(WALK_SCALE),
                jump: flags & JUMP != 0,
                thrust: thrust.value(THRUST_SCALE),
                stopping: flags & STOPPING != 0,
                hold,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A hold reads back within its quantisation, its turn's rotation vector exact both ways.
    #[test]
    fn a_hold_reads_back_within_a_millimetre_and_a_quarter_milliradian() {
        let hold = Hold {
            offset: DVec3::new(0.31, 1.27, -0.44),
            turn: DQuat::from_axis_angle(DVec3::new(1.0, 2.0, -0.5).normalize(), 2.9),
            steady: true,
        };
        let inputs = [
            AvatarInput {
                seq: 1,
                hold: Some(hold),
                ..AvatarInput::default()
            },
            AvatarInput {
                seq: 2,
                hold: Some(hold),
                ..AvatarInput::default()
            },
            AvatarInput {
                seq: 3,
                ..AvatarInput::default()
            },
        ];
        let mut w = Writer::default();
        write_inputs(&mut w, &inputs);
        let bytes = w.finish();
        let read = read_inputs(&mut Reader::new(&bytes)).unwrap();
        let back = read[1].hold.expect("held");
        assert!((back.offset - hold.offset).length() < 1e-3);
        assert!(back.turn.angle_between(hold.turn) < 3e-4, "{back:?}");
        assert!(back.steady);
        assert_eq!(read[2].hold, None);
        // Unchanged, a hold costs its flags alone.
        let mut once = Writer::default();
        write_inputs(&mut once, &inputs[..1]);
        assert!(bytes.len() < once.finish().len() + 3);
    }
}
