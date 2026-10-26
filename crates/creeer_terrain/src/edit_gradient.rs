//! The gradient of the edited density, exact: each edit's rounded max or min carried forward
//! with the gradients of its two sides (physics' contacts with the ground, `Terrain::
//! density_gradient`).
//!
//! The rounded min of Quilez (`edits::smooth_min`) is min(a, b) − k/4·h², h = (k − |a − b|)/k
//! where the two are within k of each other; its gradient is (1 − h/2)·∇low + h/2·∇high, low
//! the smaller of the two: a weighted mean of the two sides' gradients, continuous across the
//! round. So the edited field is smooth wherever its brushes' distances are, everywhere but on
//! a brush's own axis (a capsule's segment, a ball's centre), where its distance has a ridge
//! and the gradient turns from one side to the other (on the axis itself the brush gives
//! none).
//!
//! (Central differences over 1 cm, as near edits before, straddled that ridge within 1 cm of
//! the axis of a dug hole: the gradient shrank to r/1 cm there (0.027 at 0.27 mm off it),
//! and a projection step d/|∇d| threw the foot of a point 15 cm from the wall 5.4 m away, to
//! ground whose tangent plane put the point 12–20 cm inside it. A pole with its foot in a
//! 15 cm hole found 443 kJ in its contacts from one step to the next and was thrown at
//! 1,254 m/s, G2.9; ADR-0005.)

use crate::edits::{Edit, Edits, FINEST_CELL, round, smooth_min};
use creeer_math::glam::DVec3;

/// `smooth_min(a, b, k)` and its gradient, from those of a and b.
fn smooth_min_gradient((a, ga): (f64, DVec3), (b, gb): (f64, DVec3), k: f64) -> (f64, DVec3) {
    let value = smooth_min(a, b, k);
    let (low, high) = if a <= b { (ga, gb) } else { (gb, ga) };
    if k <= 0.0 {
        return (value, low);
    }
    let h = (k - (a - b).abs()).max(0.0) / k;
    (value, low * (1.0 - 0.5 * h) + high * (0.5 * h))
}

impl Edit {
    /// [`Edit::apply`] with the gradient: the edit applied at `p` to a density `density` with
    /// gradient `gradient`. The value is `apply`'s, bit for bit.
    pub fn apply_gradient(
        &self,
        p: DVec3,
        (density, gradient): (f64, DVec3),
        round: f64,
    ) -> (f64, DVec3) {
        let round = self.round(round);
        let b = self.brush();
        let distance = b.distance(p);
        // On the axis itself (its distance −r exactly) the distance has no gradient: every
        // direction across the axis is as steep. The least of its slopes there is none.
        // (`Brush::gradient` takes the world's y there: a blow struck where the last one dug
        // its brush from took the hole's surface as facing along y, found no rock behind it
        // and dug no more, `digging_tests.rs`.)
        #[allow(clippy::float_cmp)] // exactly on the axis, |p − axis| = 0
        let on_axis = distance == -b.radius();
        let brush = (distance, if on_axis { DVec3::ZERO } else { b.gradient(p) });
        match self {
            // max(d, −b) = −min(−d, b).
            Self::Dig(_) => {
                let (value, g) = smooth_min_gradient((-density, -gradient), brush, round);
                (-value, -g)
            }
            Self::Fill(..) => smooth_min_gradient((density, gradient), brush, round),
        }
    }
}

impl Edits {
    /// [`Edits::density`] with its gradient, from the generated density's and its gradient's
    /// at `p`: every edit near `p` applied in order with the finest grid's rounds.
    pub fn density_gradient(&self, p: DVec3, generated: (f64, DVec3)) -> (f64, DVec3) {
        let round = round(FINEST_CELL);
        self.near(p)
            .fold(generated, |dg, e| e.apply_gradient(p, dg, round))
    }
}
