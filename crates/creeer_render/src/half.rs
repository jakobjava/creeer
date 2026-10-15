//! f32 → IEEE 754 binary16 conversion for uploading `Rgba16Float` textures (filterable on
//! every backend, unlike 32-bit float textures).

/// Converts to half precision bits, rounding to nearest-even; overflow becomes infinity.
pub(crate) fn f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = u16::try_from((bits >> 16) & 0x8000).expect("masked");
    let exponent = i32::try_from((bits >> 23) & 0xff).expect("masked");
    let mantissa = bits & 0x007f_ffff;

    if exponent == 0xff {
        // Infinity stays infinity; NaN stays a (quiet) NaN.
        return sign | 0x7c00 | if mantissa == 0 { 0 } else { 0x0200 };
    }
    let half_exponent = exponent - 127 + 15;
    if half_exponent >= 0x1f {
        return sign | 0x7c00;
    }
    if half_exponent <= 0 {
        // Subnormal half (or zero): value = m × 2⁻²⁴.
        if half_exponent < -10 {
            return sign;
        }
        let full = mantissa | 0x0080_0000;
        let shift = u32::try_from(14 - half_exponent).expect("1..=24");
        return sign | round_shift(full, shift);
    }
    let shifted = (u32::try_from(half_exponent).expect("positive") << 10) | (mantissa >> 13);
    // Round to nearest even on the 13 dropped bits; a carry correctly bumps the exponent.
    let rest = mantissa & 0x1fff;
    let round_up = rest > 0x1000 || (rest == 0x1000 && shifted & 1 == 1);
    sign | u16::try_from(shifted + u32::from(round_up)).expect("fits in 15 bits")
}

/// `value >> shift`, rounded to nearest even.
fn round_shift(value: u32, shift: u32) -> u16 {
    let kept = value >> shift;
    let rest = value & ((1 << shift) - 1);
    let halfway = 1 << (shift - 1);
    let round_up = rest > halfway || (rest == halfway && kept & 1 == 1);
    u16::try_from(kept + u32::from(round_up)).expect("subnormal fits")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_values() {
        for (value, bits) in [
            (0.0, 0x0000),
            (-0.0, 0x8000),
            (1.0, 0x3c00),
            (0.5, 0x3800),
            (-2.0, 0xc000),
            (0.1, 0x2e66),
            (1.0 / 3.0, 0x3555),
            (65504.0, 0x7bff),
            (1.0e5, 0x7c00),
            (f32::INFINITY, 0x7c00),
            (2f32.powi(-14), 0x0400),
            (2f32.powi(-24), 0x0001),
            (2f32.powi(-26), 0x0000),
        ] {
            assert_eq!(f16_bits(value), bits, "{value}");
        }
        assert_eq!(f16_bits(f32::NAN) & 0x7c00, 0x7c00);
        assert_ne!(f16_bits(f32::NAN) & 0x03ff, 0);
    }

    #[test]
    fn ties_round_to_even() {
        // 1 + 2⁻¹¹ lies exactly between 1.0 and the next half (1 + 2⁻¹⁰): rounds to 1.0.
        assert_eq!(f16_bits(1.0 + 2f32.powi(-11)), 0x3c00);
        // 1 + 3·2⁻¹¹ lies between odd and even neighbours: rounds up to the even one.
        assert_eq!(f16_bits(1.0 + 3.0 * 2f32.powi(-11)), 0x3c02);
    }
}
