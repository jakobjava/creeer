//! A broken structure's lone pieces drawn as what they are (G2.11, ADR-0041). A built
//! structure collides by its members' halves about each node (`creeer_building`'s
//! `assemble`: a round member's half a prism of its section, a board's a box); a piece of one
//! node that breaks free is a rigid body of that hull, told with no colour. Drawn as its
//! hull's bounding ball, a collapse was a heap of large dark balls. Here the hull is fitted:
//! its axes from its corners' spread (their covariance's eigenvectors), round about one of
//! them if its corners lie at one distance from it (a rod's piece), else a block.

use crate::part_view::{Form, Made, Thing};
use creeer_math::glam::{DMat3, DQuat, DVec3};

/// Corners this much (a share) off one distance from an axis still make a round piece…
const ROUND: f64 = 0.01;
/// …if it has at least this many (two rings of six): a box's eight corners are as far from
/// each of its axes too.
const ROUND_CORNERS: usize = 12;

/// The unit eigenvectors of symmetric `m` (as columns), by Jacobi's rotations.
fn axes(m: DMat3) -> DMat3 {
    let mut a = m.to_cols_array_2d();
    let mut v = DMat3::IDENTITY.to_cols_array_2d();
    for _ in 0..32 {
        // The largest off-diagonal entry, turned to nought.
        let (p, q) = [(0, 1), (0, 2), (1, 2)]
            .into_iter()
            .max_by(|x, y| a[x.0][x.1].abs().total_cmp(&a[y.0][y.1].abs()))
            .unwrap_or((0, 1));
        if a[p][q].abs() < 1e-15 {
            break;
        }
        let theta = 0.5 * creeer_math::det::atan2(2.0 * a[p][q], a[q][q] - a[p][p]);
        let (s, c) = creeer_math::det::sin_cos(theta);
        // The matrix turned on both sides (Jᵀ·A·J): its columns, then its rows.
        for row in &mut a {
            let (akp, akq) = (row[p], row[q]);
            row[p] = c * akp - s * akq;
            row[q] = s * akp + c * akq;
        }
        let (rp, rq) = (a[p], a[q]);
        for k in 0..3 {
            a[p][k] = c * rp[k] - s * rq[k];
            a[q][k] = s * rp[k] + c * rq[k];
        }
        for row in &mut v {
            let (vp, vq) = (row[p], row[q]);
            row[p] = c * vp - s * vq;
            row[q] = s * vp + c * vq;
        }
    }
    // `v` holds the eigenvectors as its rows' p-th entries: as columns of the matrix.
    let col = |k: usize| DVec3::new(v[0][k], v[1][k], v[2][k]).normalize();
    let x = col(0);
    let y = col(1).reject_from(x).normalize();
    DMat3::from_cols(x, y, x.cross(y))
}

/// What a colourless hull of `corners` (about its body's centre `p`, turned by `q`) is drawn
/// as: a round piece of a built member, or a block; none if it has too few corners.
pub fn piece(key: u32, (p, q): (DVec3, DQuat), corners: &[DVec3]) -> Option<Thing> {
    if corners.len() < 4 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // dozens of corners
    let n = corners.len() as f64;
    let centre = corners.iter().copied().sum::<DVec3>() / n;
    let mut m = DMat3::ZERO;
    for c in corners {
        let d = *c - centre;
        m += DMat3::from_cols(d * d.x, d * d.y, d * d.z);
    }
    let frame = axes(m * (1.0 / n));
    let turn = DQuat::from_mat3(&frame);
    let local: Vec<DVec3> = corners
        .iter()
        .map(|c| frame.transpose() * (*c - centre))
        .collect();
    let (lo, hi) =
        (local.iter()).fold((DVec3::MAX, DVec3::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let half = 0.5 * (hi - lo);
    let middle = 0.5 * (hi + lo);
    // Round about one of its axes: every corner as far from it.
    let many = corners.len() >= ROUND_CORNERS;
    // (The axis its corners are most evenly far from: a short piece's corners are nearly as
    // far from an axis across it.)
    let round = (0..3)
        .filter(|_| many)
        .filter_map(|k| {
            let off = |v: &DVec3| {
                let w = *v - middle;
                let mut a = w.to_array();
                a[k] = 0.0;
                DVec3::from(a).length()
            };
            let r = local.iter().map(off).fold(0.0, f64::max);
            let spread = local.iter().map(|v| (off(v) - r).abs()).fold(0.0, f64::max);
            (r > 0.0 && spread <= ROUND * r).then_some((k, r, spread / r))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(k, r, _)| (k, r));
    let at = p + q * (centre + frame * middle);
    let (turn, form) = match round {
        // The rod's frame: its axis k as x.
        Some((k, radius)) => {
            let axis = frame.col(k);
            let rod = DQuat::from_rotation_arc(DVec3::X, axis);
            let length = 2.0 * half.to_array()[k];
            (
                q * rod,
                Form::Rod {
                    radius,
                    length,
                    taper: 1.0,
                },
            )
        }
        None => (q * turn, Form::Block(half)),
    };
    Some(Thing {
        centre: at,
        turn,
        form,
        made: Made::Built,
        key: u64::from(key),
        age: (0.0, 0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use creeer_math::det::sin_cos;

    /// A built member's half (a twelve-sided prism, turned any way) is fitted as the round
    /// piece it is; a board's half as its block.
    #[test]
    fn a_member_s_piece_is_fitted_as_what_it_is() {
        let axis = DVec3::new(0.3, 0.8, -0.5).normalize();
        let (y, z) = (
            axis.any_orthonormal_vector(),
            axis.cross(axis.any_orthonormal_vector()),
        );
        let mut prism = Vec::new();
        for s in [-0.2, 0.2] {
            for k in 0..12 {
                let (sin, cos) = sin_cos(f64::from(k) * std::f64::consts::TAU / 12.0);
                prism.push(axis * s + (y * cos + z * sin) * 0.05);
            }
        }
        let t = piece(1, (DVec3::ZERO, DQuat::IDENTITY), &prism).expect("a piece");
        let Form::Rod { radius, length, .. } = t.form else {
            panic!("not round: {:?}", t.form);
        };
        assert!(
            (radius - 0.05).abs() < 1e-6 && (length - 0.4).abs() < 1e-6,
            "{radius} {length}"
        );
        assert!((t.turn * DVec3::X).dot(axis).abs() > 0.999_999);
        let board: Vec<DVec3> = (0..8)
            .map(|k| {
                let sign = |b: i32| if k & b == 0 { -1.0 } else { 1.0 };
                axis * (0.3 * sign(1)) + y * (0.1 * sign(2)) + z * (0.025 * sign(4))
            })
            .collect();
        let t = piece(2, (DVec3::ZERO, DQuat::IDENTITY), &board).expect("a piece");
        let Form::Block(half) = t.form else {
            panic!("not a block: {:?}", t.form);
        };
        let mut sides = half.to_array();
        sides.sort_by(f64::total_cmp);
        for (got, want) in sides.iter().zip([0.025, 0.1, 0.3]) {
            assert!((got - want).abs() < 1e-6, "{sides:?}");
        }
    }
}
