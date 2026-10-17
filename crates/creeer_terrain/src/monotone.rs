//! Monotone cubic interpolation (Fritsch & Carlson 1980, with the Fritsch–Butland slopes and
//! the shape-preserving end slopes of Moler's `pchip`).
//!
//! The elevation maps noise through curves given by knots: the fraction of the surface below
//! a noise value, and the height for that fraction. Linear interpolation between knots kinks
//! the ground wherever the noise crosses a knot's value: a long, straight-looking crease that
//! shows as a line in low sunlight (measured in F4: the lines vanished with flat normals and
//! were not chunk or level borders). This interpolation goes through the same knots, never
//! overshoots between them (a monotone map stays monotone), and has a continuous slope.

/// A curve through `(x, y)` knots with strictly increasing x.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Monotone {
    xs: Vec<f64>,
    ys: Vec<f64>,
    slopes: Vec<f64>,
}

impl Monotone {
    /// The curve through `points` (sorted by x; knots with equal x keep only the first).
    pub fn new(points: impl IntoIterator<Item = (f64, f64)>) -> Self {
        let (mut xs, mut ys) = (Vec::new(), Vec::new());
        for (x, y) in points {
            if xs.last().is_some_and(|&last| x <= last) {
                continue;
            }
            xs.push(x);
            ys.push(y);
        }
        let n = xs.len();
        if n < 2 {
            return Self {
                slopes: vec![0.0; n],
                xs,
                ys,
            };
        }
        let h: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
        let d: Vec<f64> = (0..n - 1).map(|k| (ys[k + 1] - ys[k]) / h[k]).collect();
        let mut slopes = vec![0.0; n];
        for k in 1..n - 1 {
            if d[k - 1] * d[k] > 0.0 {
                // Weighted harmonic mean: never larger than three times either secant.
                let (w1, w2) = (2.0 * h[k] + h[k - 1], h[k] + 2.0 * h[k - 1]);
                slopes[k] = (w1 + w2) / (w1 / d[k - 1] + w2 / d[k]);
            }
        }
        slopes[0] = end_slope(h[0], h.get(1).copied(), d[0], d.get(1).copied());
        slopes[n - 1] = end_slope(
            h[n - 2],
            n.checked_sub(3).map(|k| h[k]),
            d[n - 2],
            n.checked_sub(3).map(|k| d[k]),
        );
        Self { xs, ys, slopes }
    }

    /// The value at `x`; constant beyond the ends.
    pub fn at(&self, x: f64) -> f64 {
        let n = self.xs.len();
        match n {
            0 => return 0.0,
            1 => return self.ys[0],
            _ => {}
        }
        if x <= self.xs[0] {
            return self.ys[0];
        }
        if x >= self.xs[n - 1] {
            return self.ys[n - 1];
        }
        let k = self.xs.partition_point(|&v| v <= x) - 1;
        let h = self.xs[k + 1] - self.xs[k];
        let t = (x - self.xs[k]) / h;
        let (t2, t3) = (t * t, t * t * t);
        // Cubic Hermite basis.
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        h00 * self.ys[k]
            + h10 * h * self.slopes[k]
            + h01 * self.ys[k + 1]
            + h11 * h * self.slopes[k + 1]
    }

    /// The knots' x values.
    pub fn xs(&self) -> &[f64] {
        &self.xs
    }

    /// The knots' y values.
    pub fn ys(&self) -> &[f64] {
        &self.ys
    }
}

/// Slope at an end knot: the three-point estimate, kept from overshooting (Moler, *Numerical
/// Computing with MATLAB*, 3.6). `h0`, `d0` are the end interval; `h1`, `d1` the next one.
fn end_slope(h0: f64, h1: Option<f64>, d0: f64, d1: Option<f64>) -> f64 {
    let (Some(h1), Some(d1)) = (h1, d1) else {
        return d0;
    };
    let m = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
    if m * d0 <= 0.0 {
        0.0
    } else if d0 * d1 <= 0.0 && m.abs() > (3.0 * d0).abs() {
        3.0 * d0
    } else {
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn knots() -> Vec<(f64, f64)> {
        // The home planet's continent curve.
        vec![
            (0.00, -3400.0),
            (0.30, -2700.0),
            (0.44, -500.0),
            (0.52, -80.0),
            (0.54, 20.0),
            (0.75, 220.0),
            (0.92, 800.0),
            (1.00, 1500.0),
        ]
    }

    #[test]
    fn passes_through_its_knots_and_never_overshoots() {
        let c = Monotone::new(knots());
        for &(x, y) in &knots() {
            assert!((c.at(x) - y).abs() < 1e-9, "at {x}");
        }
        let mut last = f64::MIN;
        for i in 0..=10_000 {
            let v = c.at(f64::from(i) / 10_000.0);
            assert!(v >= last - 1e-9, "not monotone at {i}");
            last = v;
        }
        // Between two knots it stays between their values.
        for pair in knots().windows(2) {
            let (a, b) = (pair[0], pair[1]);
            for i in 1..100 {
                let v = c.at(a.0 + (b.0 - a.0) * f64::from(i) / 100.0);
                assert!(v >= a.1.min(b.1) - 1e-9 && v <= a.1.max(b.1) + 1e-9);
            }
        }
    }

    /// No kinks: the slope just left of every interior knot equals the slope just right of
    /// it (linear interpolation jumps there, which creased the ground).
    #[test]
    fn its_slope_is_continuous_at_the_knots() {
        let c = Monotone::new(knots());
        let e = 1e-7;
        for &(x, _) in &knots()[1..knots().len() - 1] {
            let left = (c.at(x) - c.at(x - e)) / e;
            let right = (c.at(x + e) - c.at(x)) / e;
            let scale = left.abs().max(right.abs()).max(1.0);
            assert!(
                (left - right).abs() < 1e-4 * scale,
                "kink at {x}: {left} vs {right}"
            );
        }
    }

    #[test]
    fn straight_data_stays_straight_and_flat_data_flat() {
        let line = Monotone::new((0..5).map(|k| (f64::from(k), 2.0 * f64::from(k) + 1.0)));
        for i in 0..=40 {
            let x = f64::from(i) / 10.0;
            assert!((line.at(x) - (2.0 * x + 1.0)).abs() < 1e-12);
        }
        // A plateau stays a plateau: no wiggle around it.
        let step = Monotone::new([(0.0, 0.0), (1.0, 1.0), (2.0, 1.0), (3.0, 2.0)]);
        for i in 0..=10 {
            assert!((step.at(1.0 + f64::from(i) / 10.0) - 1.0).abs() < 1e-12);
        }
        // Repeated x values keep the first knot.
        let dup = Monotone::new([(0.0, 0.0), (0.0, 5.0), (1.0, 1.0)]);
        assert_eq!(dup.xs(), &[0.0, 1.0]);
    }
}
