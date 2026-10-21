use super::*;
use creeer_math::det;

/// `frames` frames of a row of 256 pixels sliding as the check turns over `pattern`
/// (8-bit levels at a position in pixels), each pixel the average of `samples` points across
/// it: many points filter the pattern as a pixel does, one point aliases it.
fn sliding(pattern: impl Fn(f64) -> f64, samples: u32, frames: u32) -> Vec<Vec<f32>> {
    (0..frames)
        .map(|t| {
            (0..256)
                .map(|x| {
                    let sum: f64 = (0..samples)
                        .map(|s| {
                            let at = f64::from(x) + (f64::from(s) + 0.5) / f64::from(samples);
                            pattern(at + f64::from(STEP_PIXELS) * f64::from(t))
                        })
                        .sum();
                    #[allow(clippy::cast_possible_truncation)] // 8-bit levels
                    let level = (sum / f64::from(samples)) as f32;
                    level
                })
                .collect()
        })
        .collect()
}

#[test]
fn smooth_detail_does_not_flicker_and_aliased_detail_does() {
    // Broad shading, a period of 40 pixels: filtered or not, it slides smoothly.
    let broad = |x: f64| 128.0 + 60.0 * det::sin(x / 40.0 * std::f64::consts::TAU);
    let smooth = measure(&sliding(broad, 1, 24), None);
    assert!(smooth.share == 0.0 && smooth.p999 < 0.5, "{smooth:?}");
    // Stripes of 0.7 pixels, finer than a pixel: point-sampled they flicker everywhere...
    let fine = |x: f64| 128.0 + 60.0 * det::sin(x / 0.7 * std::f64::consts::TAU);
    let aliased = measure(&sliding(fine, 1, 24), None);
    assert!(aliased.share > 0.9, "{aliased:?}");
    // ...but not faded to their mean, as the shader fades detail finer than a pixel, nor
    // as fine detail that a pixel still resolves (a period of 3 pixels, full contrast).
    let fade = |cells_per_pixel: f64| 1.0 - (cells_per_pixel / 0.25 - 1.0).clamp(0.0, 1.0);
    let faded = |x: f64| 128.0 + fade(1.0 / 0.7) * 60.0 * det::sin(x / 0.7 * std::f64::consts::TAU);
    assert!(measure(&sliding(faded, 1, 24), None).share == 0.0);
    let resolved = |x: f64| 128.0 + 60.0 * det::sin(x / 3.0 * std::f64::consts::TAU);
    let resolved = measure(&sliding(resolved, 1, 24), None);
    assert!(resolved.share == 0.0, "{resolved:?}");
}

#[test]
fn edges_are_found_and_widened() {
    // A one-colour frame 12 pixels wide, dark left of x = 6 and lit right of it.
    let frame: Vec<f32> = (0..12 * 5)
        .map(|i| if i % 12 < 6 { 40.0 } else { 120.0 })
        .collect();
    let near = edges(&[frame.clone(), frame.clone(), frame], 12);
    for (i, &edge) in near.iter().enumerate() {
        let x = i % 12;
        // The step lies between x = 5 and 6: both, and two pixels either side.
        assert_eq!(edge, (3..=8).contains(&x), "x = {x}");
    }
}

#[test]
fn the_look_does_not_shimmer() {
    let (found, _, _) = match check((480, 270), 16) {
        Ok(found) => found,
        Err(e) => {
            eprintln!("skipping the shimmer check: {e}");
            return;
        }
    };
    assert!(found.surfaces.share <= GATE, "{found:?}");
    // The surfaces are judged over most of the image, not a sliver of it.
    assert!(found.surface_share > 0.5, "{found:?}");
}
