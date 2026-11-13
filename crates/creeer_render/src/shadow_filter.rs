//! The shadow filter of `lighting.wgsl` (`castano7`), mirrored on the CPU to test it: Castaño's
//! 7×7 filter for The Witness ("Shadow Mapping Summary", 2013), sixteen bilinear comparison
//! taps whose places and weights make one smooth bell seven texels wide.
//!
//! What matters for the look (G2.11: shadows' edges showed the shadow map's texels as stairs):
//! as a point moves across the map, the filtered value changes continuously, so an edge
//! between lit and shadowed texels is a smooth ramp wherever it runs.

/// The four taps along one axis for a point a fraction `s` into its texel: (place in texels from
/// the base, weight); the weights sum to −52. As `castano7` in `lighting.wgsl`.
pub(crate) fn castano7(s: f32) -> [(f32, f32); 4] {
    let w0 = 5.0 * s - 6.0;
    let w1 = 11.0 * s - 28.0;
    let w2 = -(11.0 * s + 17.0);
    let w3 = -(5.0 * s + 1.0);
    [
        ((4.0 * s - 5.0) / w0 - 3.0, w0),
        ((4.0 * s - 16.0) / w1 - 1.0, w1),
        (-(7.0 * s + 5.0) / w2 + 1.0, w2),
        (-s / w3 + 3.0, w3),
    ]
}

/// The filtered value at `x` (texels; texel k spans [k, k + 1)) of a row of comparison results
/// `lit` (1 lit, 0 shadowed), as the GPU computes it in one axis: each tap a linear blend of
/// the two texels around it.
#[cfg(test)]
fn filtered(lit: &[f32], x: f32) -> f32 {
    let base = (x + 0.5).floor();
    let s = x + 0.5 - base;
    let origin = base - 0.5;
    let texel = |k: f32| {
        #[allow(clippy::cast_possible_truncation)] // small test indices
        let k = k as isize;
        usize::try_from(k).map_or(0.0, |k| lit.get(k).copied().unwrap_or(0.0))
    };
    castano7(s)
        .iter()
        .map(|&(u, w)| {
            let p = origin + u - 0.5;
            let (k, f) = (p.floor(), p - p.floor());
            w * (texel(k) * (1.0 - f) + texel(k + 1.0) * f)
        })
        .sum::<f32>()
        / -52.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_sum_to_one_and_lit_stays_lit() {
        let lit = [1.0; 32];
        for k in 0..=100u8 {
            let s = f32::from(k) / 100.0;
            let sum: f32 = castano7(s).iter().map(|t| t.1).sum();
            assert!((sum + 52.0).abs() < 1e-4, "s {s}: {sum}");
            let x = 16.0 + s;
            assert!((filtered(&lit, x) - 1.0).abs() < 1e-5);
            assert!(filtered(&[0.0; 32], x).abs() < 1e-6);
        }
    }

    /// Each tap stays between the two texels it blends, as a bilinear tap must: its place lies
    /// within the filter's seven texels, and the taps keep their order.
    #[test]
    fn taps_stay_inside_the_filter_in_order() {
        for k in 0..=100u8 {
            let s = f32::from(k) / 100.0;
            let taps = castano7(s);
            assert!(taps.windows(2).all(|w| w[0].0 < w[1].0), "{taps:?}");
            assert!(taps.iter().all(|t| (-3.5..=3.5).contains(&t.0)), "{taps:?}");
            assert!(taps.iter().all(|t| t.1 < 0.0), "{taps:?}");
        }
    }

    /// The edge of a shadow is a smooth ramp: the filtered value never jumps as the point
    /// crosses a texel (the stairs a narrow filter leaves), rises monotonically from shadow to
    /// light, and takes about seven texels to do so.
    #[test]
    fn an_edge_is_a_smooth_ramp() {
        let mut lit = [0.0f32; 40];
        for t in &mut lit[20..] {
            *t = 1.0;
        }
        let mut previous = filtered(&lit, 10.0);
        let mut x = 10.0f32;
        let mut ramp = (f32::NAN, f32::NAN);
        while x < 30.0 {
            x += 1e-3;
            let value = filtered(&lit, x);
            assert!(
                (value - previous).abs() < 2e-3,
                "jump at {x}: {previous} → {value}"
            );
            assert!(value >= previous - 1e-5, "not monotone at {x}");
            if ramp.0.is_nan() && value > 0.01 {
                ramp.0 = x;
            }
            if ramp.1.is_nan() && value > 0.99 {
                ramp.1 = x;
            }
            previous = value;
        }
        let width = ramp.1 - ramp.0;
        assert!((4.0..7.5).contains(&width), "ramp {ramp:?}");
    }
}
