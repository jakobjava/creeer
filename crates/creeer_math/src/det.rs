//! Deterministic elementary functions for simulation code.
//!
//! `f64::sin` and friends call the platform's C math library. Its results may differ in the
//! last bit between Windows, Linux and macOS, and in a chaotic simulation one bit grows into
//! a visibly different outcome. Replays, golden-hash tests and cross-platform debugging all
//! need bit-identical results, so simulation code calls these pure-Rust `libm` versions
//! instead. Clippy enforces this (see `clippy.toml`).
//!
//! `+`, `-`, `*`, `/`, `sqrt` and `mul_add` need no wrapper: IEEE 754 requires them to be
//! correctly rounded, so every platform already produces the same bits.

#![allow(missing_docs)] // Each function mirrors the `f64` method of the same name.

#[inline]
pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

#[inline]
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

#[inline]
pub fn tan(x: f64) -> f64 {
    libm::tan(x)
}

/// Returns `(sin(x), cos(x))`.
#[inline]
pub fn sin_cos(x: f64) -> (f64, f64) {
    libm::sincos(x)
}

#[inline]
pub fn asin(x: f64) -> f64 {
    libm::asin(x)
}

#[inline]
pub fn acos(x: f64) -> f64 {
    libm::acos(x)
}

#[inline]
pub fn atan(x: f64) -> f64 {
    libm::atan(x)
}

/// Four-quadrant arctangent of `y / x`, like `y.atan2(x)`.
#[inline]
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

#[inline]
pub fn sinh(x: f64) -> f64 {
    libm::sinh(x)
}

#[inline]
pub fn cosh(x: f64) -> f64 {
    libm::cosh(x)
}

#[inline]
pub fn tanh(x: f64) -> f64 {
    libm::tanh(x)
}

#[inline]
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}

#[inline]
pub fn exp2(x: f64) -> f64 {
    libm::exp2(x)
}

/// `e^x - 1`, accurate for small `x`.
#[inline]
pub fn exp_m1(x: f64) -> f64 {
    libm::expm1(x)
}

/// Natural logarithm.
#[inline]
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}

/// `ln(1 + x)`, accurate for small `x`.
#[inline]
pub fn ln_1p(x: f64) -> f64 {
    libm::log1p(x)
}

#[inline]
pub fn log2(x: f64) -> f64 {
    libm::log2(x)
}

#[inline]
pub fn log10(x: f64) -> f64 {
    libm::log10(x)
}

/// `base^exponent`, like `base.powf(exponent)`.
#[inline]
pub fn pow(base: f64, exponent: f64) -> f64 {
    libm::pow(base, exponent)
}

#[inline]
pub fn cbrt(x: f64) -> f64 {
    libm::cbrt(x)
}

/// `sqrt(x² + y²)` without intermediate overflow.
#[inline]
pub fn hypot(x: f64, y: f64) -> f64 {
    libm::hypot(x, y)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_methods)] // Comparing against std on purpose.

    use super::*;

    /// Distance between two floats in units in the last place.
    fn ulps(a: f64, b: f64) -> u64 {
        let (a, b) = (a.to_bits() as i64, b.to_bits() as i64);
        a.abs_diff(b)
    }

    fn sample_inputs() -> impl Iterator<Item = f64> {
        (-2000..=2000).map(|i| f64::from(i) * 0.0123)
    }

    /// Guards against wiring mistakes (e.g. `cos` calling `libm::sin`): each wrapper must agree
    /// with the platform implementation to within a couple of ULPs.
    #[test]
    fn wrappers_match_std_closely() {
        type Pair = (&'static str, fn(f64) -> f64, fn(f64) -> f64);
        let pairs: &[Pair] = &[
            ("sin", sin, f64::sin),
            ("cos", cos, f64::cos),
            ("tan", tan, f64::tan),
            ("atan", atan, f64::atan),
            ("sinh", sinh, f64::sinh),
            ("cosh", cosh, f64::cosh),
            ("tanh", tanh, f64::tanh),
            ("exp", exp, f64::exp),
            ("exp2", exp2, f64::exp2),
            ("exp_m1", exp_m1, f64::exp_m1),
            ("cbrt", cbrt, f64::cbrt),
        ];
        for (name, det, std) in pairs {
            for x in sample_inputs() {
                let d = ulps(det(x), std(x));
                assert!(d <= 2, "{name}({x}): {d} ulps apart");
            }
        }

        let unit = (-1000..=1000).map(|i| f64::from(i) / 1000.0);
        for x in unit {
            assert!(ulps(asin(x), x.asin()) <= 2, "asin({x})");
            assert!(ulps(acos(x), x.acos()) <= 2, "acos({x})");
        }

        let positive = (1..=4000).map(|i| f64::from(i) * 0.731);
        for x in positive {
            assert!(ulps(ln(x), x.ln()) <= 2, "ln({x})");
            assert!(ulps(ln_1p(x), x.ln_1p()) <= 2, "ln_1p({x})");
            assert!(ulps(log2(x), x.log2()) <= 2, "log2({x})");
            assert!(ulps(log10(x), x.log10()) <= 2, "log10({x})");
            assert!(ulps(pow(x, 1.37), x.powf(1.37)) <= 2, "pow({x}, 1.37)");
            assert!(ulps(hypot(x, 3.0), x.hypot(3.0)) <= 2, "hypot({x}, 3)");
            assert!(ulps(atan2(x, -2.5), x.atan2(-2.5)) <= 2, "atan2({x}, -2.5)");
        }
    }

    #[test]
    fn sin_cos_matches_separate_calls() {
        for x in sample_inputs() {
            let (s, c) = sin_cos(x);
            assert_eq!(s.to_bits(), sin(x).to_bits());
            assert_eq!(c.to_bits(), cos(x).to_bits());
        }
    }

    /// Bit-exact reference values. If this test fails on any platform, simulation results are
    /// no longer portable and replays and golden hashes will diverge.
    #[test]
    fn golden_bits_are_identical_on_every_platform() {
        let values = golden_values();
        if std::env::var_os("CREEER_PRINT_GOLDEN").is_some() {
            for (name, v) in &values {
                println!("{:#018x}, // {name} = {v:e}", v.to_bits());
            }
        }
        assert_eq!(values.len(), GOLDEN_BITS.len());
        for ((name, got), expected) in values.iter().zip(GOLDEN_BITS) {
            assert_eq!(
                got.to_bits(),
                expected,
                "{name} = {got:e} (bits {:#018x}), expected bits {expected:#018x}",
                got.to_bits()
            );
        }
    }

    /// Inputs chosen to exercise the hard paths: large-argument reduction, tiny arguments,
    /// near-overflow values.
    fn golden_values() -> Vec<(&'static str, f64)> {
        vec![
            ("sin(1)", sin(1.0)),
            ("cos(1)", cos(1.0)),
            ("tan(1)", tan(1.0)),
            ("sin(1e6)", sin(1e6)),
            ("sin(1e22)", sin(1e22)),
            ("asin(0.3)", asin(0.3)),
            ("acos(0.3)", acos(0.3)),
            ("atan(3)", atan(3.0)),
            ("atan2(1,-2)", atan2(1.0, -2.0)),
            ("sinh(0.7)", sinh(0.7)),
            ("cosh(0.7)", cosh(0.7)),
            ("tanh(0.7)", tanh(0.7)),
            ("exp(0.5)", exp(0.5)),
            ("exp(-20)", exp(-20.0)),
            ("exp2(0.3)", exp2(0.3)),
            ("exp_m1(1e-10)", exp_m1(1e-10)),
            ("ln(10)", ln(10.0)),
            ("ln_1p(1e-10)", ln_1p(1e-10)),
            ("log2(3)", log2(3.0)),
            ("log10(7)", log10(7.0)),
            ("pow(2.5,3.7)", pow(2.5, 3.7)),
            ("pow(1.0000001,1e7)", pow(1.000_000_1, 1e7)),
            ("cbrt(10)", cbrt(10.0)),
            ("hypot(3e200,4e200)", hypot(3e200, 4e200)),
        ]
    }

    // Generated once on Windows x86-64 (`CREEER_PRINT_GOLDEN=1 cargo test`); must hold on
    // every platform. Never regenerate these to make a failure go away.
    const GOLDEN_BITS: [u64; 24] = [
        0x3feaed548f090cee, // sin(1) = 8.414709848078965e-1
        0x3fe14a280fb5068c, // cos(1) = 5.403023058681398e-1
        0x3ff8eb245cbee3a6, // tan(1) = 1.5574077246549023e0
        0xbfd6664b2568d867, // sin(1e6) = -3.4999350217129294e-1
        0xbfeb453ab76bf397, // sin(1e22) = -8.522008497671888e-1
        0x3fd380159e14f6ff, // asin(0.3) = 3.046926540153975e-1
        0x3ff441f5ecbeef59, // acos(0.3) = 1.2661036727794992e0
        0x3ff3fc176b7a8560, // atan(3) = 1.2490457723982544e0
        0x40056c6e7397f5ae, // atan2(1,-2) = 2.677945044588987e0
        0x3fe8465153d5bdbc, // sinh(0.7) = 7.585837018395334e-1
        0x3ff4152c1862342f, // cosh(0.7) = 1.255169005630943e0
        0x3fe356fb17af2e90, // tanh(0.7) = 6.043677771171634e-1
        0x3ffa61298e1e069c, // exp(0.5) = 1.6487212707001282e0
        0x3e21b48655f37267, // exp(-20) = 2.061153622438558e-9
        0x3ff3b2c47bff8329, // exp2(0.3) = 1.2311444133449163e0
        0x3ddb7cdfd9dda4e3, // exp_m1(1e-10) = 1.00000000005e-10
        0x40026bb1bbb55516, // ln(10) = 2.302585092994046e0
        0x3ddb7cdfd9d1d693, // ln_1p(1e-10) = 9.999999999500001e-11
        0x3ff95c01a39fbd68, // log2(3) = 1.584962500721156e0
        0x3feb0b0b0b78cc3f, // log10(7) = 8.450980400142568e-1
        0x403dac93f32d18dd, // pow(2.5,3.7) = 2.967413253642086e1
        0x4005bf0a790ce6f2, // pow(1.0000001,1e7) = 2.7182816941320818e0
        0x40013c484138704f, // cbrt(10) = 2.154434690031884e0
        0x699a20df0dcd3af0, // hypot(3e200,4e200) = 4.9999999999999995e200
    ];
}
