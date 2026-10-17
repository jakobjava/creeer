//! Exact integrals over a triangle of a linear function's positive part: the pressure of a
//! patch (`patch.rs`) over each triangle of its domain.
//!
//! The part of the triangle where f > 0 is a polygon (the triangle, a corner of it, or the
//! triangle less a corner), integrated directly as a fan of triangles. (As the whole triangle
//! less the corner where f < 0, a thin pressed strip along an edge was the difference of two
//! nearly equal numbers: its energy and forces were noise in their leading digits, F5.)

/// A corner of the part where f > 0: its weights in the triangle, and f there.
type Corner = ([f64; 3], f64);

/// The polygon where f > 0 (at most four corners: a triangle cut by a line), its corners in
/// order around it, and how many there are. On the stack: a rock pile's faces integrate a
/// hundred thousand partly pressed triangles a tick, and allocating their polygons cost a
/// fifth of the faces' laws.
fn positive_part(f: [f64; 3]) -> ([Corner; 4], usize) {
    let mut polygon = [([0.0; 3], 0.0); 4];
    let mut len = 0;
    let mut push = |c: Corner| {
        polygon[len] = c;
        len += 1;
    };
    for i in 0..3 {
        let j = (i + 1) % 3;
        let mut corner = [0.0; 3];
        corner[i] = 1.0;
        if f[i] > 0.0 {
            push((corner, f[i]));
        }
        if (f[i] > 0.0) != (f[j] > 0.0) {
            // Where the edge from i to j crosses f = 0: s from i, 1 − s = f[j]/(f[j] − f[i]).
            let s = f[i] / (f[i] - f[j]);
            let mut cut = [0.0; 3];
            cut[i] = f[j] / (f[j] - f[i]);
            cut[j] = s;
            push((cut, 0.0));
        }
    }
    (polygon, len)
}

/// The fan of triangles of `polygon`: each sub-triangle's area per the triangle's area, and
/// its corners.
fn fan(polygon: &[Corner]) -> impl Iterator<Item = (f64, [Corner; 3])> + '_ {
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
    let (polygon, len) = positive_part(f);
    cut_square(&polygon[..len], area)
}

/// [`square_integral`] over the positive part `polygon` of a triangle cut by f = 0.
fn cut_square(polygon: &[Corner], area: f64) -> (f64, [f64; 3]) {
    let (mut integral, mut gradient) = (0.0, [0.0; 3]);
    for (ratio, corners) in fan(polygon) {
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

#[cfg(test)]
/// ∫ φₐ·φᵦ over the part of a triangle where f > 0 (φ the corners' linear weights): half
/// the Hessian of [`square_integral`], which is continuous as the cut moves.
pub fn pressed_mass(f: [f64; 3], area: f64) -> [[f64; 3]; 3] {
    let (polygon, len) = positive_part(f);
    cut_mass(&polygon[..len], area)
}

/// [`pressed_mass`] over the positive part `polygon` of a triangle cut by f = 0. Symmetric:
/// each pair once (its products and sums are the same numbers either way round).
fn cut_mass(polygon: &[Corner], area: f64) -> [[f64; 3]; 3] {
    let mut mass = [[0.0; 3]; 3];
    for (ratio, corners) in fan(polygon) {
        let sub = area * ratio;
        let w = corners.map(|c| c.0);
        let sums: [f64; 3] = std::array::from_fn(|a| w.iter().map(|p| p[a]).sum::<f64>());
        for a in 0..3 {
            for b in a..3 {
                let products: f64 = w.iter().map(|p| p[a] * p[b]).sum();
                mass[a][b] += sub / 12.0 * (products + sums[a] * sums[b]);
                if b > a {
                    mass[b][a] = mass[a][b];
                }
            }
        }
    }
    mass
}

/// [`square_integral`] and [`pressed_mass`] of a triangle pressed in part, its positive part
/// found once for both.
pub fn cut_square_and_mass(f: [f64; 3], area: f64) -> ((f64, [f64; 3]), [[f64; 3]; 3]) {
    let (polygon, len) = positive_part(f);
    (
        cut_square(&polygon[..len], area),
        cut_mass(&polygon[..len], area),
    )
}

/// The area of the part of a triangle of area `area` where f > 0 (f linear, `f` at the
/// corners).
pub fn pressed_area(f: [f64; 3], area: f64) -> f64 {
    if f.iter().all(|&x| x > 0.0) {
        return area;
    }
    let (polygon, len) = positive_part(f);
    fan(&polygon[..len]).map(|(ratio, _)| area * ratio).sum()
}
