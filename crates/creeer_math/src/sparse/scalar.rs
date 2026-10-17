//! The numbers a sparse system is made of: real (DC circuits, heat, flow) or complex (AC
//! phasors). Complex arithmetic is written out here, in a fixed order, so results have the same
//! bits on every platform.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// A number a sparse matrix can hold.
pub trait Scalar:
    Copy
    + PartialEq
    + std::fmt::Debug
    + Send
    + Sync
    + Add<Output = Self>
    + Sub<Output = Self>
    + Mul<Output = Self>
    + Div<Output = Self>
    + Neg<Output = Self>
    + AddAssign
    + SubAssign
    + 'static
{
    /// 0.
    const ZERO: Self;
    /// 1.
    const ONE: Self;
    /// Its size, for choosing pivots (|x| for reals; |re| + |im| for complex numbers, which
    /// needs no square root and orders pivots as well).
    fn magnitude(self) -> f64;
    /// Whether it is finite.
    fn is_finite(self) -> bool;
}

impl Scalar for f64 {
    const ZERO: Self = 0.0;
    const ONE: Self = 1.0;
    fn magnitude(self) -> f64 {
        self.abs()
    }
    fn is_finite(self) -> bool {
        f64::is_finite(self)
    }
}

/// A complex number re + i·im.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    /// Real part.
    pub re: f64,
    /// Imaginary part.
    pub im: f64,
}

impl Complex {
    /// re + i·im.
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    /// The imaginary unit.
    pub const I: Self = Self::new(0.0, 1.0);

    /// Its conjugate.
    #[must_use]
    pub const fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }

    /// |z|² (no square root).
    pub fn norm_squared(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    /// |z|.
    pub fn abs(self) -> f64 {
        crate::det::hypot(self.re, self.im)
    }

    /// Its argument (rad).
    pub fn arg(self) -> f64 {
        crate::det::atan2(self.im, self.re)
    }

    /// r·e^(iφ).
    pub fn polar(r: f64, phi: f64) -> Self {
        let (s, c) = crate::det::sin_cos(phi);
        Self::new(r * c, r * s)
    }
}

impl From<f64> for Complex {
    fn from(re: f64) -> Self {
        Self::new(re, 0.0)
    }
}

impl Add for Complex {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.re + o.re, self.im + o.im)
    }
}

impl Sub for Complex {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for Complex {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        Self::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}

impl Div for Complex {
    type Output = Self;
    /// Smith's algorithm: no overflow for large components.
    fn div(self, o: Self) -> Self {
        if o.re.abs() >= o.im.abs() {
            let r = o.im / o.re;
            let d = o.re + o.im * r;
            Self::new((self.re + self.im * r) / d, (self.im - self.re * r) / d)
        } else {
            let r = o.re / o.im;
            let d = o.re * r + o.im;
            Self::new((self.re * r + self.im) / d, (self.im * r - self.re) / d)
        }
    }
}

impl Neg for Complex {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.re, -self.im)
    }
}

impl AddAssign for Complex {
    fn add_assign(&mut self, o: Self) {
        *self = *self + o;
    }
}

impl SubAssign for Complex {
    fn sub_assign(&mut self, o: Self) {
        *self = *self - o;
    }
}

impl Scalar for Complex {
    const ZERO: Self = Self::new(0.0, 0.0);
    const ONE: Self = Self::new(1.0, 0.0);
    fn magnitude(self) -> f64 {
        self.re.abs() + self.im.abs()
    }
    fn is_finite(self) -> bool {
        self.re.is_finite() && self.im.is_finite()
    }
}
