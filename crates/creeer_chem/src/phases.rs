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
        condensed.push(j);
        moles[j] = 0.0;
        return true;
    }
    false
}
