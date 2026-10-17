//! Exact integrals over a triangle of a linear function's positive part: the pressure of a
//! patch (`patch.rs`) over each triangle of its domain.
//!
//! The part of the triangle where f > 0 is a polygon (the triangle, a corner of it, or the
//! triangle less a corner), integrated directly as a fan of triangles. (As the whole triangle
//! less the corner where f < 0, a thin pressed strip along an edge was the difference of two
//! nearly equal numbers: its energy and forces were noise in their leading digits, F5.)

/// The polygon where f > 0, its corners in the triangle's weights (in order around it),
/// with f there.
fn positive_part(f: [f64; 3]) -> Vec<([f64; 3], f64)> {
    let mut polygon = Vec::with_capacity(4);
    for i in 0..3 {
        let j = (i + 1) % 3;
        let mut corner = [0.0; 3];
        corner[i] = 1.0;
        if f[i] > 0.0 {
            polygon.push((corner, f[i]));
        }
        if (f[i] > 0.0) != (f[j] > 0.0) {
            // Where the edge from i to j crosses f = 0: s from i, 1 − s = f[j]/(f[j] − f[i]).
            let s = f[i] / (f[i] - f[j]);
            let mut cut = [0.0; 3];
            cut[i] = f[j] / (f[j] - f[i]);
            cut[j] = s;
            polygon.push((cut, 0.0));
        }
    }
    polygon
}

/// The fan of triangles of `polygon`: each sub-triangle's area per the triangle's area, and
/// its corners.
fn fan(polygon: &[([f64; 3], f64)]) -> impl Iterator<Item = (f64, [([f64; 3], f64); 3])> + '_ {
    (1..polygon.len().saturating_sub(1)).map(move |k| {
        let corners = [polygon[0], polygon[k], polygon[k + 1]];
        let [a, b, c] = corners.map(|p| p.0);
        // The area ratio is |det| of the corners' weights.
        let det = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
            + a[2] * (b[0] * c[1] - b[1] * c[0]);
        (det.abs(), corners)
    })
}

/// ∫ max(f, 0)² over a triangle of area `area` on which f is linear with values `f` at the
/// corners, and its gradient with respect to `f`. The integrand vanishes where the cut
/// moves, so the gradient is 2·∫ max(f, 0)·φᵢ (φᵢ the corners' linear weights).
pub fn square_integral(f: [f64; 3], area: f64) -> (f64, [f64; 3]) {
    if f.iter().all(|&x| x > 0.0) {
        let [a, b, c] = f;
        let integral = area / 6.0 * (a * a + b * b + c * c + a * b + b * c + c * a);
        let gradient = [
            area / 6.0 * (2.0 * a + b + c),
            area / 6.0 * (2.0 * b + c + a),
            area / 6.0 * (2.0 * c + a + b),
        ];
        return (integral, gradient);
    }
    let (mut integral, mut gradient) = (0.0, [0.0; 3]);
    for (ratio, corners) in fan(&positive_part(f)) {
        let sub = area * ratio;
        let v = corners.map(|c| c.1);
        integral += sub / 6.0
            * (v[0] * v[0] + v[1] * v[1] + v[2] * v[2] + v[0] * v[1] + v[1] * v[2] + v[2] * v[0]);
        // 2·∫ f·φ with ∫ f·g = A/12·(Σ fₘgₘ + Σf·Σg) for linear f, g.
        let sum: f64 = v.iter().sum();
        for (i, g) in gradient.iter_mut().enumerate() {
            let phi = corners.map(|c| c.0[i]);
            let products: f64 = v.iter().zip(&phi).map(|(a, b)| a * b).sum();
            *g += sub / 6.0 * (products + sum * phi.iter().sum::<f64>());
        }
    }
    (integral, gradient)
}

/// ∫ φₐ·φᵦ over the part of a triangle where f > 0 (φ the corners' linear weights): half
/// the Hessian of [`square_integral`], which is continuous as the cut moves.
pub fn pressed_mass(f: [f64; 3], area: f64) -> [[f64; 3]; 3] {
    let mut mass = [[0.0; 3]; 3];
    for (ratio, corners) in fan(&positive_part(f)) {
        let sub = area * ratio;
        let w = corners.map(|c| c.0);
        for (a, row) in mass.iter_mut().enumerate() {
            for (b, m) in row.iter_mut().enumerate() {
                let products: f64 = w.iter().map(|p| p[a] * p[b]).sum();
                let sums =
                    w.iter().map(|p| p[a]).sum::<f64>() * w.iter().map(|p| p[b]).sum::<f64>();
                *m += sub / 12.0 * (products + sums);
            }
        }
    }
    mass
}

/// The area of the part of a triangle of area `area` where f > 0 (f linear, `f` at the
/// corners).
pub fn pressed_area(f: [f64; 3], area: f64) -> f64 {
    if f.iter().all(|&x| x > 0.0) {
        return area;
    }
    fan(&positive_part(f)).map(|(ratio, _)| area * ratio).sum()
}
