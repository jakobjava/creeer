//! The joint minimisation of a group's incremental potential (see `group.rs`).

use super::coupled::Coupled;
use super::group::{Coupling, Member, states, trial_states};
use super::lbfgs::Lbfgs;
use super::rigid_system::RigidSystem;
use super::solver::Refactoring;
use creeer_math::glam::DVec3;
use std::time::Instant;

/// The member's residual (−β²·∇G, fixed axes zeroed) including the coupling's forces.
fn residuals(members: &mut [Member<'_>], coupling: &dyn Coupling, gradient: &mut [Vec<DVec3>]) {
    for g in gradient.iter_mut() {
        g.fill(DVec3::ZERO);
    }
    coupling.add_gradient(&states(members), gradient);
    for ((s, v), g) in members.iter_mut().zip(gradient.iter()) {
        let scale = v.work.beta * v.work.beta;
        for (block, (d, g)) in v.work.direction.iter_mut().zip(g).enumerate() {
            *d -= *g * s.block_mask(block) * scale;
        }
    }
}

/// The rigid bodies' matrices made again with the coupling's stiffness, and their joint system
/// (`old` its last).
fn remake(
    old: Option<RigidSystem>,
    members: &[Member<'_>],
    coupling: &dyn Coupling,
    rigid: &mut [Option<[[f64; 6]; 6]>],
) -> Option<RigidSystem> {
    for (m, slot) in rigid.iter_mut().enumerate() {
        *slot = members[m]
            .1
            .rigid_matrix(members[m].0, coupling.rigid_stiffness(m));
    }
    RigidSystem::again(old, members, coupling, rigid)
}

/// How far `direction` goes down the objective: the residual (the negative gradient) along it.
fn downhill(residual: &[DVec3], direction: &[DVec3]) -> f64 {
    residual.iter().zip(direction).map(|(r, d)| r.dot(*d)).sum()
}

/// Newton/L-BFGS iterations with line search on the group's incremental potential, until
/// the estimated remaining error is below the tolerance.
pub(super) fn minimise(members: &mut [Member<'_>], coupling: &mut dyn Coupling) {
    let t = Instant::now();
    let mut gradient: Vec<Vec<DVec3>> = members
        .iter()
        .map(|(_, v)| vec![DVec3::ZERO; v.work.direction.len()])
        .collect();
    let mut objective = coupling.energy(&states(members));
    for (s, v) in members.iter_mut() {
        let (g, elastic) = v.work.evaluate(s, false, true);
        objective += g;
        v.work.current_elastic = elastic;
        v.work.gather(s);
        v.update_turns(s);
    }
    residuals(members, coupling, &mut gradient);
    // A NaN compares false with everything: the line search would find no decrease and the
    // step would stop where it began, as if converged (a NaN dashpot did, F5).
    debug_assert!(objective.is_finite(), "the step's objective is {objective}");
    members[0].1.timings.residual += t.elapsed();
    let setup = crate::profile::start();
    let mut rigid: Vec<Option<[[f64; 6]; 6]>> = (0..members.len())
        .map(|m| {
            members[m]
                .1
                .rigid_matrix(members[m].0, coupling.rigid_stiffness(m))
        })
        .collect();
    let mut system = RigidSystem::new(members, coupling, &rigid);
    // Structures take the contacts' stiffness into their matrix (refactorising only when
    // it changed materially, judged without the joints a joint solve takes exactly).
    let joints = Coupled::joints(members, coupling, &rigid, system.as_ref());
    for (m, (s, v)) in members.iter_mut().enumerate() {
        let mut blocks = coupling.node_stiffness(m);
        blocks.extend(s.dashpot_blocks(v.work.beta, v.work.dashpot_scale));
        blocks.extend(s.added_blocks(v.work.beta));
        let exempt: Vec<usize> = joints.iter().filter(|j| j.0 == m).map(|j| j.1).collect();
        if !s.elements.is_empty() && v.use_contact_stiffness(s, blocks, &exempt) {
            v.update_turns(s);
        }
    }
    // Rigid bodies resting only on structures: solved jointly with them (`coupled.rs`).
    let coupled = Coupled::new(members, coupling, &rigid, system.as_ref());
    setup.stop(crate::profile::Slot::Setup);
    let offsets: Vec<usize> = members
        .iter()
        .scan(0, |offset, (_, v)| {
            let start = *offset;
            *offset += v.work.direction.len();
            Some(start)
        })
        .collect();
    // Not below what the positions can resolve: a few roundings of the farthest coordinate (a
    // contact barely touching asked for 10⁻¹⁹ m, and the step went on to its 500th iteration).
    let far = members
        .iter()
        .flat_map(|(s, _)| s.position.iter())
        .fold(1.0, |far: f64, p| far.max(p.abs().max_element()));
    let (tolerance, max_iterations) = (
        members[0]
            .1
            .tolerance
            .min(coupling.tolerance())
            .max(4.0 * f64::EPSILON * far),
        members[0].1.max_iterations,
    );
    let mut lbfgs = Lbfgs::default();
    let mut residual: Vec<DVec3> = members
        .iter()
        .flat_map(|(_, v)| v.work.direction.iter().copied())
        .collect();
    let mut direction = residual.clone();
    let (mut previous_move, mut previous_rate) = (f64::INFINITY, None);
    let mut lag_updates = 0;
    // A group of rigid bodies alone (no structure) takes its contacts' stiffness at each
    // iteration's state into its matrices: Newton's method proper. Their friction's stiffness
    // turns with the slip (`Coupling::relinearise`), and frozen at the step's start it left
    // piles of sliding rocks creeping, 50–400 iterations a step (F6).
    let relinear = coupled.is_none() && rigid.iter().all(Option::is_some);
    let mut refactored = false;
    for iteration in 0..max_iterations {
        // (Not at the very end: within ten tolerances the last iterate's matrix serves as well,
        // and making it again cost a stack of crates a tenth of its step.)
        if relinear
            && iteration > 0
            && previous_move > 10.0 * tolerance
            && coupling.relinearise(&states(members))
        {
            let timer = crate::profile::start();
            system = remake(system.take(), members, coupling, &mut rigid);
            timer.stop(crate::profile::Slot::Relinearise);
            #[cfg(feature = "profile")]
            if std::env::var_os("CREEER_CHECK_HESSIAN").is_some() {
                let label = format!("iteration {iteration}");
                super::hessian_check::check(members, coupling, &label);
                super::hessian_check::check_own(members, coupling, &label);
            }
        }
        if lag_updates > 0 {
            crate::profile::add(crate::profile::Slot::LagIterations, 1);
        }
        let t = Instant::now();
        for (s, v) in members.iter_mut() {
            v.last_iterations += 1;
            v.iterations_total += 1;
            if v.refactoring == Refactoring::EveryIteration && !s.elements.is_empty() {
                v.refactor(s);
                v.update_turns(s);
                lbfgs.clear();
            }
        }
        direction.copy_from_slice(&residual);
        lbfgs.direction(&mut direction, |all| {
            let kept = coupled.as_ref().map(|c| c.residuals(all, &offsets));
            for (m, ((s, v), matrix)) in members.iter_mut().zip(&rigid).enumerate() {
                if system.as_ref().is_some_and(|j| j.contains(m))
                    || coupled.as_ref().is_some_and(|c| c.contains(m))
                {
                    continue;
                }
                let range = offsets[m]..offsets[m] + v.work.direction.len();
                v.precondition(s, &mut all[range], matrix.as_ref());
            }
            if let Some(joint) = system.as_mut() {
                joint.solve(all, &offsets);
            }
            if let (Some(c), Some(kept)) = (coupled.as_ref(), kept.as_ref()) {
                c.solve(all, kept, &offsets);
            }
        });
        // A direction uphill would fail every line search, and a failed search reads as
        // convergence (the joint solve of rigid bodies on structures gave such directions while
        // its correction for the contacts was scaled wrong: a frame froze 3 mm above its piers
        // under a block, a bridge stopped ringing at once, F6). Should one occur again, each
        // member's own matrix, positive definite, gives the direction, and L-BFGS restarts.
        if downhill(&residual, &direction) <= 0.0 {
            direction.copy_from_slice(&residual);
            for (m, ((s, v), matrix)) in members.iter_mut().zip(&rigid).enumerate() {
                let range = offsets[m]..offsets[m] + v.work.direction.len();
                v.precondition(s, &mut direction[range], matrix.as_ref());
            }
            lbfgs.clear();
        }
        let mut offset = 0;
        for (_, v) in members.iter_mut() {
            let len = v.work.direction.len();
            v.work
                .direction
                .copy_from_slice(&direction[offset..offset + len]);
            offset += len;
        }
        members[0].1.timings.solve += t.elapsed();
        let longest = direction.iter().map(|d| d.length()).fold(0.0, f64::max);

        // Line search on the total.
        let t = Instant::now();
        // The first trial goes as far as the first contact now open closes, if that is short of
        // the full step (`Coupling::closing`).
        let mut alpha = coupling
            .closing(&states(members), &direction, &offsets)
            .map_or(1.0, |a| a.clamp(1e-3, 1.0));
        let first_alpha = alpha;
        let mut accepted = None;
        // The objective's slope along the direction at its start (the residual is −β²·∇G).
        let beta = members[0].1.work.beta;
        let slope = -downhill(&residual, &direction) / (beta * beta);
        for halving in 0..=12 {
            crate::profile::add(crate::profile::Slot::Trials, 1);
            let mut members_value = 0.0;
            let mut elastic = Vec::with_capacity(members.len());
            for (s, v) in members.iter_mut() {
                v.work.set_trial(s, alpha);
                let (g, e) = v.work.evaluate(s, true, halving == 0);
                members_value += g;
                elastic.push(e);
            }
            let value = members_value + coupling.energy(&trial_states(members));
            // (A shorter step no lower is rounding's: taken, it moved nothing, again and again.)
            #[allow(clippy::float_cmp)] // the full step, no higher
            if value < objective || (value == objective && halving == 0) {
                accepted = Some((value, members_value, elastic, halving == 0));
                break;
            }
            // Back to the minimum of the parabola through the start (its value and slope) and
            // this trial, kept between a tenth and a half of this step: one trial back where
            // halving took several (a rock pile's steps took 2.5 trials an iteration).
            let curve = (value - objective - slope * alpha) / (alpha * alpha);
            let back = if curve > 0.0 && slope < 0.0 {
                -slope / (2.0 * curve)
            } else {
                0.5 * alpha
            };
            alpha = back.clamp(0.1 * alpha, 0.5 * alpha);
            if alpha < 1.0 / 4096.0 {
                break;
            }
        }
        members[0].1.timings.objective += t.elapsed();
        let Some((value, members_value, elastic, first_trial)) = accepted else {
            // G is at its minimum to machine precision: converged, so the lagged quantities
            // are refreshed as below. (Stopping here without the refresh left a box starting
            // exactly on a ramp without friction for its first step, found in F5.)
            if lag_updates == 3 || !coupling.update_lag(&states(members)) {
                break;
            }
            lag_updates += 1;
            // Go on from here with the refreshed quantities, as at the start.
            objective = coupling.energy(&states(members));
            for (s, v) in members.iter_mut() {
                let (g, elastic) = v.work.evaluate(s, false, true);
                objective += g;
                v.work.current_elastic = elastic;
                v.work.gather(s);
            }
            residuals(members, coupling, &mut gradient);
            residual = members
                .iter()
                .flat_map(|(_, v)| v.work.direction.iter().copied())
                .collect();
            (previous_move, previous_rate) = (f64::INFINITY, None);
            lbfgs.clear();
            continue;
        };
        objective = value;
        let own = members_value;
        let mut lag_changed = false;
        for ((s, v), e) in members.iter_mut().zip(elastic) {
            v.work.current_elastic = e;
            std::mem::swap(&mut s.position, &mut v.work.trial_x);
            std::mem::swap(&mut s.orientation, &mut v.work.trial_q);
        }
        // Remaining error: with convergence at rate ρ (measured from the last two steps)
        // about step·ρ/(1 − ρ); before a rate is known, the step itself. A rate after one
        // that showed no convergence (ρ ≥ 0.5) is not trusted: one sudden small step after
        // steps that did not shrink says little (a ball rolling over bumps moved 0.23 mm,
        // 0.23 mm, then 2 µm, and stopped with its force 4 % off, F5). A step the line
        // search cut short is short for that, not for being near the end: the whole step
        // proposed is what remains (one cut to 1/32 stopped another step 1 % off).
        let moved = alpha * longest;
        let rate = previous_move.is_finite().then(|| moved / previous_move);
        #[allow(clippy::float_cmp)] // the full step, exactly
        let remaining = match rate {
            _ if alpha != 1.0 => longest,
            Some(r) if r < 0.5 && previous_rate.is_none_or(|p| p < 0.5) => moved * r / (1.0 - r),
            _ => moved,
        };
        (previous_move, previous_rate) = (moved, rate);
        if remaining < tolerance {
            // Converged with the lagged quantities: refresh them (at most 3 times) and go on
            // if they moved (IPC's lagged friction iterations).
            if lag_updates == 3 || !coupling.update_lag(&states(members)) {
                break;
            }
            crate::profile::add(crate::profile::Slot::LagUpdates, 1);
            lag_updates += 1;
            objective = own + coupling.energy(&states(members));
            lag_changed = true;
            (previous_move, previous_rate) = (f64::INFINITY, None);
            lbfgs.clear();
        }
        // (Backtracking, not a step stopped where a contact closes, says the matrix is off.)
        if alpha < 0.25 * first_alpha && !refactored {
            refactored = true;
            for (s, v) in members.iter_mut() {
                if v.refactoring == Refactoring::WhenSlow && !s.elements.is_empty() {
                    v.refactor(s);
                    v.update_turns(s);
                }
            }
            lbfgs.clear();
        }
        let t = Instant::now();
        let step: Vec<DVec3> = direction.iter().map(|d| *d * alpha).collect();
        for (s, v) in members.iter_mut() {
            // The line search computed the forces at its first trial: if that was
            // accepted, only the per-node sums remain to be done.
            if first_trial {
                v.work.gather(s);
            } else {
                v.work.residual(s);
            }
        }
        residuals(members, coupling, &mut gradient);
        let new: Vec<DVec3> = members
            .iter()
            .flat_map(|(_, v)| v.work.direction.iter().copied())
            .collect();
        let change = residual
            .iter()
            .zip(&new)
            .map(|(old, new)| *old - *new)
            .collect();
        residual = new;
        if !lag_changed {
            lbfgs.push(step, change);
        }
        members[0].1.timings.residual += t.elapsed();
    }
}
