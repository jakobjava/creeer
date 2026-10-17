//! Chemical equilibrium by Gibbs energy minimisation under element balance: the method of
//! NASA's CEA program (S. Gordon and B. J. McBride, NASA RP-1311, 1994, ch. 2–3).
//!
//! Newton's method on the elements' Lagrange multipliers π (per RT), the condensed phases'
//! moles, the log of the gas moles and, for an adiabatic problem, the log of the
//! temperature; each gas's log moles follow from them. Steps are limited as RP-1311 limits
//! them (eq. 3.1–3.3: no species' moles more than e² times larger in a step, trace species
//! kept from leaping). Once converged, condensed phases are tested: one whose moles went
//! negative is removed, one that would lower the Gibbs energy (μ°/RT < Σ a·π) and whose
//! data cover the temperature is added, and the system converged again.

use crate::dense::solve;
use crate::system::System;
use creeer_materials::species::R;
use creeer_math::det;

/// ln(10⁸): a gas below 10⁻⁸ of the gas moles is a trace (RP-1311's SIZE).
const SIZE: f64 = 18.420_681;
/// Iterations at most per set of condensed phases, and sets at most.
const ITERATIONS: usize = 80;
const SETS: usize = 40;

/// What is held fixed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fixed {
    /// Temperature (K) and pressure (Pa).
    Tp { t: f64, p: f64 },
    /// Enthalpy (J, of the whole mixture) and pressure (Pa): adiabatic, at constant pressure.
    Hp { h: f64, p: f64 },
}

/// An equilibrium found.
#[derive(Clone, Debug, PartialEq)]
pub struct Equilibrium {
    /// Moles of each species of the system.
    pub moles: Vec<f64>,
    pub t: f64,
    pub p: f64,
    /// The elements' Lagrange multipliers over RT (each element's chemical potential).
    pub pi: Vec<f64>,
    pub iterations: usize,
}

/// What an iteration did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Going,
    Converged,
    /// A condensed phase ran out and left.
    Emptied,
}

/// The iteration's state.
struct State<'a> {
    sys: &'a System,
    /// Elements present (indices into the system's), their moles wanted.
    elements: Vec<usize>,
    b0: Vec<f64>,
    /// Gases that can form (all their elements present) and the condensed phases in.
    gases: Vec<usize>,
    condensed: Vec<usize>,
    /// Log moles of each gas (by species index), condensed moles, log gas moles, log T.
    ln_n: Vec<f64>,
    moles: Vec<f64>,
    ln_total: f64,
    ln_t: f64,
    p: f64,
    adiabatic: Option<f64>,
    pi: Vec<f64>,
}

impl State<'_> {
    /// One Newton iteration; whether it had converged before its step.
    fn iterate(&mut self) -> Result<Step, String> {
        let (sys, t) = (self.sys, det::exp(self.ln_t));
        let total = det::exp(self.ln_total);
        let (ne, nc) = (self.elements.len(), self.condensed.len());
        let hp = self.adiabatic.is_some();
        let m = ne + nc + 1 + usize::from(hp);
        let ln_p = det::ln(self.p / creeer_materials::species::STANDARD_PRESSURE);
        // μ/RT, H/RT and c_p/R of each species in play.
        let mut mu = vec![0.0; sys.species.len()];
        let mut h = vec![0.0; sys.species.len()];
        let mut cp = vec![0.0; sys.species.len()];
        for &j in self.gases.iter().chain(&self.condensed) {
            let s = &sys.species[j];
            h[j] = s.h_rt(t);
            cp[j] = s.cp_r(t);
            mu[j] = h[j] - s.s_r(t)
                + if j < sys.gases {
                    self.ln_n[j] - self.ln_total + ln_p
                } else {
                    0.0
                };
        }
        let n = |j: usize| self.moles[j];
        let a = |i: usize, j: usize| sys.atoms[self.elements[i]][j];
        let mut mat = vec![0.0; m * m];
        let mut rhs = vec![0.0; m];
        let (row_n, row_t) = (ne + nc, ne + nc + 1);
        for i in 0..ne {
            let b: f64 = self
                .gases
                .iter()
                .chain(&self.condensed)
                .map(|&j| a(i, j) * n(j))
                .sum();
            rhs[i] = self.b0[i] - b;
            for &j in &self.gases {
                let anj = a(i, j) * n(j);
                for k in 0..ne {
                    mat[i * m + k] += a(k, j) * anj;
                }
                mat[i * m + row_n] += anj;
                rhs[i] += anj * mu[j];
                if hp {
                    mat[i * m + row_t] += anj * h[j];
                }
            }
            for (c, &j) in self.condensed.iter().enumerate() {
                mat[i * m + ne + c] = a(i, j);
            }
        }
        for (c, &j) in self.condensed.iter().enumerate() {
            let r = ne + c;
            for i in 0..ne {
                mat[r * m + i] = a(i, j);
            }
            rhs[r] = mu[j];
            if hp {
                mat[r * m + row_t] = h[j];
            }
        }
        let gas: f64 = self.gases.iter().map(|&j| n(j)).sum();
        for i in 0..ne {
            mat[row_n * m + i] = self.gases.iter().map(|&j| a(i, j) * n(j)).sum();
        }
        mat[row_n * m + row_n] = gas - total;
        rhs[row_n] = total - gas + self.gases.iter().map(|&j| n(j) * mu[j]).sum::<f64>();
        if let Some(target) = self.adiabatic {
            mat[row_n * m + row_t] = self.gases.iter().map(|&j| n(j) * h[j]).sum();
            for i in 0..ne {
                mat[row_t * m + i] = self.gases.iter().map(|&j| a(i, j) * n(j) * h[j]).sum();
            }
            for (c, &j) in self.condensed.iter().enumerate() {
                mat[row_t * m + ne + c] = h[j];
            }
            mat[row_t * m + row_n] = mat[row_n * m + row_t];
            let all = self.gases.iter().chain(&self.condensed);
            mat[row_t * m + row_t] = all.clone().map(|&j| n(j) * cp[j]).sum::<f64>()
                + self.gases.iter().map(|&j| n(j) * h[j] * h[j]).sum::<f64>();
            let enthalpy: f64 = all.map(|&j| n(j) * h[j]).sum();
            rhs[row_t] = target / (R * t) - enthalpy
                + self.gases.iter().map(|&j| n(j) * h[j] * mu[j]).sum::<f64>();
        }
        let (copy, given) = (mat.clone(), rhs.clone());
        if !solve(&mut mat, &mut rhs, m) {
            // Elements the gases hold only in fixed proportion (CO₂ alone: carbon and oxygen
            // always 1 : 2) leave their multipliers determined only in combination, which is
            // all the moles depend on: a touch on the multipliers' diagonal picks one.
            (mat, rhs) = (copy, given);
            let scale = (0..ne).map(|i| mat[i * m + i].abs()).fold(1e-300, f64::max);
            for i in 0..ne {
                mat[i * m + i] += 1e-12 * scale;
            }
            if !solve(&mut mat, &mut rhs, m) {
                return Err("a singular equilibrium system".into());
            }
        }
        self.pi.clone_from(&rhs[..ne].to_vec());
        let (dln_total, dln_t) = (rhs[row_n], if hp { rhs[row_t] } else { 0.0 });
        let dln: Vec<(usize, f64)> = self
            .gases
            .iter()
            .map(|&j| {
                let pi: f64 = (0..ne).map(|i| a(i, j) * rhs[i]).sum();
                (j, -mu[j] + pi + dln_total + h[j] * dln_t)
            })
            .collect();
        // Converged? (RP-1311 eq. 3.5.)
        let tol = 0.5e-5 * gas.max(1e-300);
        let converged = dln.iter().all(|&(j, d)| n(j) * d.abs() <= tol)
            && (0..nc).all(|c| rhs[ne + c].abs() <= tol)
            && total * dln_total.abs() <= tol
            && dln_t.abs() <= 1e-4;
        // The step (RP-1311 eq. 3.1–3.3).
        let mut big = 5.0 * dln_t.abs().max(dln_total.abs());
        let mut lambda2 = f64::INFINITY;
        for &(j, d) in &dln {
            let relative = self.ln_n[j] - self.ln_total;
            if relative > -SIZE && d > 0.0 {
                big = big.max(d);
            } else if relative <= -SIZE && d >= 0.0 && (d - dln_total).abs() > 0.0 {
                lambda2 = lambda2.min(((-relative - 9.210_340_4) / (d - dln_total)).abs());
            }
        }
        let mut lambda = 1.0f64
            .min(if big > 2.0 { 2.0 / big } else { 1.0 })
            .min(lambda2);
        // A phase the step would take below nothing stops at nothing and leaves (calcite
        // above its decomposition temperature: else Newton pushes it down for ever). One just
        // added, at nothing, may pass through it: it is judged once converged.
        let mut emptied = None;
        for (c, &j) in self.condensed.iter().enumerate() {
            let d = rhs[ne + c];
            if self.moles[j] > 0.0 && d < 0.0 && lambda * d < -self.moles[j] {
                lambda = self.moles[j] / -d;
                emptied = Some(c);
            }
        }
        for (j, d) in dln {
            self.ln_n[j] = (self.ln_n[j] + lambda * d).max(-700.0);
            self.moles[j] = det::exp(self.ln_n[j]);
        }
        for (c, &j) in self.condensed.iter().enumerate() {
            self.moles[j] += lambda * rhs[ne + c];
        }
        self.ln_total += lambda * dln_total;
        self.ln_t += lambda * dln_t;
        if let Some(c) = emptied {
            let j = self.condensed.remove(c);
            self.moles[j] = 0.0;
            return Ok(Step::Emptied);
        }
        Ok(if converged {
            Step::Converged
        } else {
            Step::Going
        })
    }
}

/// The equilibrium of `feed` (moles of each of the system's species: what goes in) under
/// `fixed`, continuing from `warm` (the moles and temperature of an equilibrium near it: a
/// reactor's last) or from scratch: every gas alike (the CEA program's start), the feed's
/// condensed phases in (a solid put in is there), and for each element no gas carries the
/// phase that holds it most stably (lowest G per atom of it).
///
/// # Errors
/// If the system is singular or does not converge.
pub fn equilibrate(
    sys: &System,
    feed: &[f64],
    fixed: Fixed,
    warm: Option<(&[f64], f64)>,
) -> Result<Equilibrium, String> {
    let b0 = sys.element_moles(feed);
    let start = warm;
    let largest = b0.iter().fold(0.0f64, |m, b| m.max(*b));
    let elements: Vec<usize> = (0..b0.len()).filter(|&i| b0[i] > 1e-12 * largest).collect();
    let possible =
        |j: usize| (0..sys.elements.len()).all(|i| sys.atoms[i][j] == 0.0 || elements.contains(&i));
    let gases: Vec<usize> = (0..sys.gases).filter(|&j| possible(j)).collect();
    let candidates: Vec<usize> = (sys.gases..sys.species.len())
        .filter(|&j| possible(j))
        .collect();
    let (p, adiabatic, t0) = match fixed {
        Fixed::Tp { t, p } => (p, None, t),
        Fixed::Hp { h, p } => (p, Some(h), start.map_or(3800.0, |s| s.1)),
    };
    let mut moles = vec![0.0; sys.species.len()];
    match start {
        Some((m, _)) => moles.copy_from_slice(m),
        None => gases
            .iter()
            .for_each(|&j| moles[j] = 0.1 / gases.len() as f64),
    }
    for &j in &gases {
        moles[j] = moles[j].max(1e-30 * largest);
    }
    let mut state = State {
        sys,
        b0: elements.iter().map(|&i| b0[i]).collect(),
        elements,
        ln_n: moles.iter().map(|&n| det::ln(n.max(1e-300))).collect(),
        ln_total: det::ln(gases.iter().map(|&j| moles[j]).sum::<f64>().max(1e-300)),
        moles,
        condensed: Vec::new(),
        gases,
        ln_t: det::ln(t0),
        p,
        adiabatic,
        pi: Vec::new(),
    };
    let present = |j: usize| start.map_or(feed[j], |s| s.0[j]) > 0.0;
    state.condensed = candidates.iter().copied().filter(|&j| present(j)).collect();
    for &j in &state.condensed {
        state.moles[j] = start.map_or(feed[j], |s| s.0[j]);
    }
    // An element no gas carries is held by a condensed phase from the start.
    for (e, &i) in state.elements.clone().iter().enumerate() {
        if state.gases.iter().any(|&j| sys.atoms[i][j] > 0.0)
            || state.condensed.iter().any(|&j| sys.atoms[i][j] > 0.0)
        {
            continue;
        }
        let stable = (candidates.iter().copied())
            .filter(|&j| sys.atoms[i][j] > 0.0 && sys.species[j].covers(t0))
            .min_by(|&a, &b| {
                (sys.species[a].g_rt(t0) / sys.atoms[i][a])
                    .total_cmp(&(sys.species[b].g_rt(t0) / sys.atoms[i][b]))
            });
        if let Some(j) = stable {
            state.condensed.push(j);
            state.moles[j] = state.b0[e] / sys.atoms[i][j];
        }
    }
    let mut iterations = 0;
    for _ in 0..SETS {
        let mut step = Step::Going;
        for _ in 0..ITERATIONS {
            iterations += 1;
            step = state.iterate()?;
            if step != Step::Going {
                break;
            }
        }
        match step {
            Step::Going => return Err(format!("no convergence in {ITERATIONS} iterations")),
            // A phase ran out: converge again without it.
            Step::Emptied => continue,
            Step::Converged => {}
        }
        if !crate::phases::revise(
            &mut state.condensed,
            &mut state.moles,
            &candidates,
            sys,
            &state.elements,
            &state.pi,
            det::exp(state.ln_t),
        ) {
            return Ok(Equilibrium {
                moles: state.moles,
                t: det::exp(state.ln_t),
                p,
                pi: state.pi,
                iterations,
            });
        }
    }
    Err(format!("condensed phases not settled in {SETS} sets"))
}
