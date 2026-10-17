//! Diagnostic (feature `profile`, `CREEER_CHECK_HESSIAN=1`, difference step `CREEER_CHECK_STEP`):
//! a coupling's matrix blocks for
//! rigid bodies against central differences of its exact gradient, and its gradient against
//! differences of its energy, at a step's own states. Prints the worst relative errors.
// Row and column indices are the 6×6 matrices' own (translation, rotation) coordinates.
#![allow(clippy::needless_range_loop)]

use super::coupling::{Coupling, MemberState};
use super::group::Member;
use crate::so3;
use creeer_math::glam::{DQuat, DVec3};

/// Positions and orientations owned, to perturb.
type Owned = Vec<(Vec<DVec3>, Vec<DQuat>)>;

fn owned(members: &[Member<'_>]) -> Owned {
    members
        .iter()
        .map(|(s, _)| (s.position.clone(), s.orientation.clone()))
        .collect()
}

fn view(o: &Owned) -> Vec<MemberState<'_>> {
    o.iter()
        .map(|(x, q)| MemberState {
            position: x,
            orientation: q,
        })
        .collect()
}

/// Moves member `m`'s node 0 by `e` along degree of freedom `k` (translation, then a rotation
/// on the left in the world frame).
fn moved(base: &Owned, m: usize, k: usize, e: f64) -> Owned {
    let mut o = base.clone();
    let mut d = DVec3::ZERO;
    d[k % 3] = e;
    if k < 3 {
        o[m].0[0] += d;
    } else {
        o[m].1[0] = (so3::exp(d) * o[m].1[0]).normalize();
    }
    o
}

fn gradient(coupling: &dyn Coupling, o: &Owned, members: &[Member<'_>]) -> Vec<[f64; 6]> {
    let mut g: Vec<Vec<DVec3>> = members
        .iter()
        .map(|(s, _)| vec![DVec3::ZERO; 2 * s.node_count()])
        .collect();
    coupling.add_gradient(&view(o), &mut g);
    g.iter()
        .zip(members)
        .map(|(g, (s, _))| {
            let (t, r) = (g[0], g[s.node_count()]);
            [t.x, t.y, t.z, r.x, r.y, r.z]
        })
        .collect()
}

/// Checks the coupling's rigid blocks and gradient at the members' states.
pub(super) fn check(members: &[Member<'_>], coupling: &dyn Coupling, label: &str) {
    let rigid: Vec<usize> = (0..members.len())
        .filter(|&m| members[m].0.elements.is_empty() && members[m].0.node_count() == 1)
        .collect();
    if rigid.len() != members.len() {
        return;
    }
    let base = owned(members);
    let g0 = gradient(coupling, &base, members);
    let couplings = coupling.rigid_coupling();
    let ee: f64 = std::env::var("CREEER_CHECK_STEP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1e-6);
    let e = 0.1 * ee;
    let mut worst_block: f64 = 0.0;
    let mut worst_cross: f64 = 0.0;
    let mut worst_gradient: f64 = 0.0;
    let mut worst_rate: f64 = 0.0;
    let mut detail = String::new();
    for &m in &rigid {
        let own = coupling.rigid_stiffness(m);
        // Column k of the Hessian: the change of every member's gradient as m moves along k.
        let mut fd = vec![[[0.0; 6]; 6]; members.len()];
        for k in 0..6 {
            let (plus, minus) = (moved(&base, m, k, e), moved(&base, m, k, -e));
            let (gp, gm) = (
                gradient(coupling, &plus, members),
                gradient(coupling, &minus, members),
            );
            for (n, block) in fd.iter_mut().enumerate() {
                for r in 0..6 {
                    block[r][k] = (gp[n][r] - gm[n][r]) / (2.0 * e);
                }
            }
            // The gradient against the energy's difference.
            let (ep, em) = (
                coupling.energy(&view(&moved(&base, m, k, ee))),
                coupling.energy(&view(&moved(&base, m, k, -ee))),
            );
            let slope = (ep - em) / (2.0 * ee);
            let scale = g0[m].iter().fold(1.0f64, |a, v| a.max(v.abs()));
            worst_gradient = worst_gradient.max((slope - g0[m][k]).abs() / scale);
        }
        // The rate a Newton iteration with the used matrix converges at, for this body alone:
        // the spectral radius of A_true⁻¹·(A_used − A_true), A = mass + β²·(the coupling's).
        if let Some(used) = members[m].1.rigid_matrix(members[m].0, own) {
            let b2 = members[m].1.work.beta * members[m].1.work.beta;
            let truth: [[f64; 6]; 6] = std::array::from_fn(|r| {
                std::array::from_fn(|c| used[r][c] - b2 * own[r][c] + b2 * fd[m][r][c])
            });
            let rate = spectral_rate(&truth, &used);
            worst_rate = worst_rate.max(rate);
        }
        let scale = fd[m].iter().flatten().fold(1e-30f64, |a, v| a.max(v.abs()));
        let error = (0..6)
            .flat_map(|r| (0..6).map(move |c| (r, c)))
            .map(|(r, c)| (own[r][c] - fd[m][r][c]).abs() / scale)
            .fold(0.0, f64::max);
        if error > 1e-3 {
            let parts: Vec<String> = [(0, 0, "tt"), (0, 3, "tr"), (3, 3, "rr")]
                .iter()
                .map(|&(r0, c0, name)| {
                    let mut err = 0.0f64;
                    for r in r0..r0 + 3 {
                        for c in c0..c0 + 3 {
                            err = err.max((own[r][c] - fd[m][r][c]).abs());
                        }
                    }
                    format!("{name} {:.2e}", err / scale)
                })
                .collect();
            detail += &format!(
                "  member {m}: own block off by {error:.2e} ({})\n",
                parts.join(", ")
            );
        }
        worst_block = worst_block.max(error);
        for n in rigid.iter().copied().filter(|&n| n != m) {
            let block = couplings.iter().filter(|c| c.0.0 == n && c.1.0 == m).fold(
                [[0.0; 6]; 6],
                |mut a, c| {
                    for r in 0..6 {
                        for k in 0..6 {
                            a[r][k] += c.2[r][k];
                        }
                    }
                    a
                },
            );
            let block =
                couplings
                    .iter()
                    .filter(|c| c.0.0 == m && c.1.0 == n)
                    .fold(block, |mut a, c| {
                        for r in 0..6 {
                            for k in 0..6 {
                                a[r][k] += c.2[k][r];
                            }
                        }
                        a
                    });
            for r in 0..6 {
                for k in 0..6 {
                    worst_cross = worst_cross.max((block[r][k] - fd[n][r][k]).abs() / scale);
                }
            }
        }
    }
    eprintln!(
        "{label}: {} members, blocks off by {worst_block:.2e}, couplings {worst_cross:.2e}, gradient {worst_gradient:.2e}\n{detail}",
        members.len()
    );
}

/// The spectral radius of truth⁻¹·(used − truth), by power iteration.
fn spectral_rate(truth: &[[f64; 6]; 6], used: &[[f64; 6]; 6]) -> f64 {
    let solve = |b: [f64; 6]| -> [f64; 6] {
        let mut a = *truth;
        let mut x = b;
        for k in 0..6 {
            let p = (k..6)
                .max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))
                .unwrap_or(k);
            a.swap(k, p);
            x.swap(k, p);
            for i in k + 1..6 {
                let f = a[i][k] / a[k][k];
                for j in k..6 {
                    a[i][j] -= f * a[k][j];
                }
                x[i] -= f * x[k];
            }
        }
        for k in (0..6).rev() {
            let s: f64 = (k + 1..6).map(|j| a[k][j] * x[j]).sum();
            x[k] = (x[k] - s) / a[k][k];
        }
        x
    };
    let mut v = [1.0, 0.7, 0.5, 0.3, 0.2, 0.1];
    let mut rate = 0.0;
    for _ in 0..60 {
        let d: [f64; 6] =
            std::array::from_fn(|r| (0..6).map(|c| (used[r][c] - truth[r][c]) * v[c]).sum());
        let w = solve(d);
        let norm = w.iter().map(|x| x * x).sum::<f64>().sqrt();
        if norm == 0.0 || !norm.is_finite() {
            return 0.0;
        }
        let old = v.iter().map(|x| x * x).sum::<f64>().sqrt();
        rate = norm / old;
        v = w.map(|x| x / norm);
    }
    rate
}

/// The full objective's per-body rate: each rigid member's own terms (inertia, drag, loads)
/// differenced too, with the coupling's, against the matrix the step uses.
pub(super) fn check_own(members: &mut [Member<'_>], coupling: &dyn Coupling, label: &str) {
    if members
        .iter()
        .any(|(s, _)| !(s.elements.is_empty() && s.node_count() == 1))
    {
        return;
    }
    let e: f64 = std::env::var("CREEER_CHECK_STEP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1e-6)
        * 0.1;
    let mut worst: f64 = 0.0;
    let mut worst_own: f64 = 0.0;
    for m in 0..members.len() {
        let own = coupling.rigid_stiffness(m);
        let Some(used) = members[m].1.rigid_matrix(members[m].0, own) else {
            continue;
        };
        let b2 = members[m].1.work.beta * members[m].1.work.beta;
        // −d(direction)/dq: direction is −β²·∇G of the member's own terms.
        let mut fd = [[0.0; 6]; 6];
        let (x0, q0) = (members[m].0.position[0], members[m].0.orientation[0]);
        for k in 0..6 {
            let mut columns = [[0.0; 6]; 2];
            for (side, sign) in [(0, 1.0), (1, -1.0)] {
                let (s, v) = &mut members[m];
                let mut d = DVec3::ZERO;
                d[k % 3] = sign * e;
                if k < 3 {
                    s.position[0] = x0 + d;
                } else {
                    s.orientation[0] = (so3::exp(d) * q0).normalize();
                }
                v.work.residual(s);
                let (t, r) = (v.work.direction[0], v.work.direction[1]);
                columns[side] = [t.x, t.y, t.z, r.x, r.y, r.z];
                s.position[0] = x0;
                s.orientation[0] = q0;
            }
            for r in 0..6 {
                fd[r][k] = -(columns[0][r] - columns[1][r]) / (2.0 * e);
            }
        }
        {
            let (s, v) = &mut members[m];
            v.work.residual(s);
        }
        // The coupling's part, as in `check`.
        let base = owned(members);
        let mut coupling_fd = [[0.0; 6]; 6];
        for k in 0..6 {
            let (gp, gm) = (
                gradient(coupling, &moved(&base, m, k, e), members),
                gradient(coupling, &moved(&base, m, k, -e), members),
            );
            for r in 0..6 {
                coupling_fd[r][k] = (gp[m][r] - gm[m][r]) / (2.0 * e);
            }
        }
        let own_used: [[f64; 6]; 6] =
            std::array::from_fn(|r| std::array::from_fn(|c| used[r][c] - b2 * own[r][c]));
        let truth: [[f64; 6]; 6] =
            std::array::from_fn(|r| std::array::from_fn(|c| fd[r][c] + b2 * coupling_fd[r][c]));
        let rate = spectral_rate(&truth, &used);
        if rate > 0.3 {
            let fmt = |a: &[[f64; 6]; 6]| -> String {
                a.iter()
                    .map(|row| {
                        row.iter()
                            .map(|v| format!("{v:10.3e}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .collect::<Vec<_>>()
                    .join(
                        "
      ",
                    )
            };
            eprintln!(
                "  rate {rate:.3} member {m}
    used:
      {}
    truth:
      {}
    own used:
      {}
    own truth:
      {}",
                fmt(&used),
                fmt(&truth),
                fmt(&own_used),
                fmt(&fd)
            );
        }
        worst = worst.max(rate);
        // The member's own terms alone.
        worst_own = worst_own.max(spectral_rate(&fd, &own_used));
    }
    eprintln!(
        "{label}: {} members, full rate {worst:.3}, own terms' rate {worst_own:.3}",
        members.len()
    );
}
