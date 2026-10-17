//! The exponential of a small dense matrix: scaling and squaring with a Taylor series
//! (the norm scaled below ½, 20 terms: below f64 rounding), in plain arithmetic so it is the
//! same on every platform.

/// A dense square matrix, row-major.
#[derive(Clone, Debug, PartialEq)]
pub struct Dense {
    pub n: usize,
    pub a: Vec<f64>,
}

impl Dense {
    pub fn zeros(n: usize) -> Self {
        Self {
            n,
            a: vec![0.0; n * n],
        }
    }

    pub fn identity(n: usize) -> Self {
        let mut m = Self::zeros(n);
        for i in 0..n {
            m.a[i * n + i] = 1.0;
        }
        m
    }

    pub fn at(&mut self, i: usize, j: usize) -> &mut f64 {
        &mut self.a[i * self.n + j]
    }

    /// `self` times `other` into `out` (its buffer reused).
    fn mul_into(&self, other: &Self, out: &mut Self) {
        let n = self.n;
        out.a.iter_mut().for_each(|v| *v = 0.0);
        for i in 0..n {
            for k in 0..n {
                let x = self.a[i * n + k];
                if x == 0.0 {
                    continue;
                }
                for j in 0..n {
                    out.a[i * n + j] += x * other.a[k * n + j];
                }
            }
        }
    }

    /// `self` times the vector `v`.
    pub fn apply(&self, v: &[f64]) -> Vec<f64> {
        let n = self.n;
        (0..n)
            .map(|i| (0..n).map(|j| self.a[i * n + j] * v[j]).sum())
            .collect()
    }

    /// The largest row sum of magnitudes (the ∞-norm).
    fn norm(&self) -> f64 {
        (0..self.n)
            .map(|i| {
                self.a[i * self.n..(i + 1) * self.n]
                    .iter()
                    .map(|x| x.abs())
                    .sum()
            })
            .fold(0.0, f64::max)
    }

    /// e^(t·self).
    pub fn exp(&self, t: f64) -> Self {
        let norm = self.norm() * t.abs();
        let mut squarings = 0u32;
        let mut scale = t;
        let mut scaled_norm = norm;
        while scaled_norm > 0.5 {
            scale *= 0.5;
            scaled_norm *= 0.5;
            squarings += 1;
        }
        let mut x = self.clone();
        x.a.iter_mut().for_each(|v| *v *= scale);
        // Taylor: I + X + X²/2 + …, term by term until a term is below rounding (‖X‖ ≤ ½:
        // at most 18 of them).
        let mut result = Self::identity(self.n);
        let mut term = Self::identity(self.n);
        let mut next = Self::zeros(self.n);
        for k in 1..=24 {
            term.mul_into(&x, &mut next);
            let inv = 1.0 / f64::from(k);
            next.a.iter_mut().for_each(|v| *v *= inv);
            std::mem::swap(&mut term, &mut next);
            for (r, t) in result.a.iter_mut().zip(&term.a) {
                *r += t;
            }
            if term.norm() < 1e-18 {
                break;
            }
        }
        let mut copy = Self::zeros(self.n);
        for _ in 0..squarings {
            copy.a.copy_from_slice(&result.a);
            copy.mul_into(&copy, &mut result);
        }
        result
    }
}

/// A linear system's path [s, 1, ∫s] from z₀: by the matrix exponential, or in closed form
/// for one state (ds/dt = a·s + c: s = s₀·eˣ + c·t·φ₁(x), ∫s = s₀·t·φ₁(x) + c·t²·φ₂(x),
/// x = a·t), the common case of one tank whose outflow follows its level.
#[derive(Clone, Debug)]
pub enum Path {
    Matrix(Dense),
    Scalar { a: f64, c: f64 },
}

impl Path {
    /// The system of matrix `m` ([s, 1, ∫s] with `states` states).
    pub fn of(m: Dense, states: usize) -> Self {
        if states == 1 {
            let (a, c) = (m.a[0], m.a[1]);
            Self::Scalar { a, c }
        } else {
            Self::Matrix(m)
        }
    }

    /// [s, 1, ∫s] at `t` from `z0`.
    pub fn at(&self, t: f64, z0: &[f64]) -> Vec<f64> {
        let mut z = Vec::with_capacity(z0.len());
        self.write(t, z0, &mut z);
        z
    }

    /// [s, 1, ∫s] at `t` from `z0`, into `z`.
    pub fn write(&self, t: f64, z0: &[f64], z: &mut Vec<f64>) {
        z.clear();
        match self {
            Self::Matrix(m) => z.extend(m.exp(t).apply(z0)),
            &Self::Scalar { a, c } => {
                let x = a * t;
                let (p1, p2) = phis(x);
                let s0 = z0[0];
                z.extend([
                    s0 * creeer_math::det::exp(x) + c * t * p1,
                    1.0,
                    s0 * t * p1 + c * t * t * p2,
                ]);
            }
        }
    }
}

/// φ₁(x) = (eˣ − 1)/x and φ₂(x) = (eˣ − 1 − x)/x², accurate near 0 (by their series).
fn phis(x: f64) -> (f64, f64) {
    if x.abs() < 0.5 {
        // φ₂ = Σ xⁿ/(n+2)!, φ₁ = 1 + x·φ₂: 18 terms are below rounding at |x| < ½.
        let (mut term, mut p2) = (0.5, 0.5);
        for n in 1..18 {
            term *= x / f64::from(n + 2);
            p2 += term;
        }
        (1.0 + x * p2, p2)
    } else {
        let m1 = creeer_math::det::exp_m1(x);
        (m1 / x, (m1 - x) / (x * x))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exponential_of_a_diagonal_and_of_a_rotation() {
        let mut d = Dense::zeros(2);
        *d.at(0, 0) = -3.0;
        *d.at(1, 1) = 0.5;
        let e = d.exp(2.0);
        assert!((e.a[0] - creeer_math::det::exp(-6.0f64)).abs() < 1e-16);
        assert!((e.a[3] / creeer_math::det::exp(1.0) - 1.0).abs() < 1e-14);
        // [[0, −1], [1, 0]]·t: a rotation by t.
        let mut r = Dense::zeros(2);
        *r.at(0, 1) = -1.0;
        *r.at(1, 0) = 1.0;
        let e = r.exp(1.2);
        assert!(
            (e.a[0] - creeer_math::det::cos(1.2)).abs() < 1e-14
                && (e.a[2] - creeer_math::det::sin(1.2)).abs() < 1e-14
        );
        // Large: many squarings.
        let e = d.exp(40.0);
        assert!(
            (e.a[0] / creeer_math::det::exp(-120.0f64) - 1.0).abs() < 1e-11,
            "{}",
            e.a[0]
        );
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    /// The closed form for one state against the matrix exponential, over small and large
    /// exponents of either sign.
    #[test]
    fn one_state_in_closed_form_is_its_exponential() {
        for (a, c) in [
            (-0.01, 2.0),
            (-3.0, 0.5),
            (0.2, -1.0),
            (-1e-9, 1.0),
            (0.0, 3.0),
        ] {
            let mut m = Dense::zeros(3);
            *m.at(0, 0) = a;
            *m.at(0, 1) = c;
            *m.at(2, 0) = 1.0;
            let z0 = [7.0, 1.0, 0.0];
            for t in [1e-3, 0.4, 3.0, 50.0, 400.0] {
                let exact = Path::Scalar { a, c }.at(t, &z0);
                let matrix = Path::Matrix(m.clone()).at(t, &z0);
                for (x, y) in exact.iter().zip(&matrix) {
                    let scale = x.abs().max(y.abs()).max(1.0);
                    assert!(
                        (x - y).abs() <= 1e-12 * scale,
                        "a {a} c {c} t {t}: {x} against {y}"
                    );
                }
            }
        }
    }
}
