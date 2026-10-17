//! Which condensed phases an equilibrium holds (RP-1311 §3.5): after each convergence, a
//! phase whose moles went negative, or whose data no longer cover the temperature, leaves;
//! else the phase that would lower the Gibbs energy most joins, the one with the most
//! negative μ°/RT − Σᵢ aᵢ·πᵢ (its chemical potential below what its elements are worth in
//! the mixture). One change at a time, the system converged again after each.

use crate::system::System;

/// Below this μ°/RT − Σ a·π a phase joins: above rounding, far below any reaction worth
/// its name (10⁻⁶ RT is 0.01 J/mol at 1000 K).
const JOIN: f64 = -1e-6;

/// Revises the condensed phases in `condensed` (their moles in `moles`) among `candidates`
/// at temperature `t`, with the elements' multipliers `pi` (for the system's elements
/// `elements`). Whether anything changed.
pub(crate) fn revise(
    condensed: &mut Vec<usize>,
    moles: &mut [f64],
    candidates: &[usize],
    sys: &System,
    elements: &[usize],
    pi: &[f64],
    t: f64,
) -> bool {
    // A phase out of its range turns into the same substance's phase that covers the
    // temperature (α-iron into γ above 1184 K), its moles with it (RP-1311 §3.5).
    for k in 0..condensed.len() {
        let j = condensed[k];
        if sys.species[j].covers(t) {
            continue;
        }
        let same = |c: &usize| {
            sys.species[*c].formula == sys.species[j].formula && sys.species[*c].covers(t)
        };
        if let Some(&next) = candidates
            .iter()
            .find(|c| same(c) && !condensed.contains(c))
        {
            condensed[k] = next;
            moles[next] = moles[j];
            moles[j] = 0.0;
            return true;
        }
    }
    // A phase gone (negative moles) or out of its range leaves, the most negative first.
    let leaving = condensed
        .iter()
        .enumerate()
        .filter(|&(_, &j)| moles[j] <= 0.0 || !sys.species[j].covers(t))
        .min_by(|a, b| moles[*a.1].total_cmp(&moles[*b.1]))
        .map(|(k, _)| k);
    if let Some(k) = leaving {
        let j = condensed.remove(k);
        moles[j] = 0.0;
        return true;
    }
    let worth = |j: usize| -> f64 {
        sys.species[j].g_rt(t)
            - (0..elements.len())
                .map(|i| sys.atoms[elements[i]][j] * pi[i])
                .sum::<f64>()
    };
    let joining = candidates
        .iter()
        .copied()
        .filter(|j| !condensed.contains(j) && sys.species[*j].covers(t))
        .map(|j| (j, worth(j)))
        .filter(|&(_, w)| w < JOIN)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((j, _)) = joining {
        match dependence(condensed, j, sys, elements) {
            // Its composition is the phases' in already (a_j = Σ c_k·a_k): it cannot join them,
            // only replace one (the phase rule: iron, wüstite and magnetite coexist at one
            // temperature only). The one that runs out first as it forms goes (the simplex's
            // ratio test), the elements where they were.
            Some(c) => {
                let out = (0..condensed.len())
                    .filter(|&k| c[k] > 1e-12)
                    .min_by(|&a, &b| {
                        (moles[condensed[a]] / c[a]).total_cmp(&(moles[condensed[b]] / c[b]))
                    });
                let Some(out) = out else { return false };
                let made = (moles[condensed[out]] / c[out]).max(0.0);
                for (k, &p) in condensed.iter().enumerate() {
                    moles[p] -= c[k] * made;
                }
                moles[condensed[out]] = 0.0;
                condensed[out] = j;
                moles[j] = made;
            }
            None => {
                condensed.push(j);
                moles[j] = 0.0;
            }
        }
        return true;
    }
    false
}

/// The coefficients c with a_j = Σ c_k·a_k over the condensed phases `condensed`, if phase
/// `j`'s composition lies in theirs (to rounding); `None` if it adds an independent one.
fn dependence(condensed: &[usize], j: usize, sys: &System, elements: &[usize]) -> Option<Vec<f64>> {
    let n = condensed.len();
    if n == 0 {
        return None;
    }
    let a = |i: usize, k: usize| sys.atoms[elements[i]][k];
    // Least squares: (AᵀA)·c = Aᵀ·a_j.
    let mut m = vec![0.0; n * n];
    let mut b = vec![0.0; n];
    for (r, &p) in condensed.iter().enumerate() {
        for (s, &q) in condensed.iter().enumerate() {
            m[r * n + s] = (0..elements.len()).map(|i| a(i, p) * a(i, q)).sum();
        }
        b[r] = (0..elements.len()).map(|i| a(i, p) * a(i, j)).sum();
    }
    if !crate::dense::solve(&mut m, &mut b, n) {
        return None;
    }
    let residual: f64 = (0..elements.len())
        .map(|i| {
            let fit: f64 = condensed.iter().zip(&b).map(|(&p, c)| a(i, p) * c).sum();
            (fit - a(i, j)) * (fit - a(i, j))
        })
        .sum();
    let size: f64 = (0..elements.len()).map(|i| a(i, j) * a(i, j)).sum();
    (residual <= 1e-18 * size).then_some(b)
}
