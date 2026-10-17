//! A solver as a save holds it: its settings, its history (what the next step continues
//! from), and what its matrix was built from, so that the loaded solver builds the very
//! matrix the saved one had and the world goes on bit for bit (`world/save.rs`).

use super::super::Structure;
use super::super::element::Stiffness;
use super::super::solver::{ImplicitSolver, Refactoring, Scheme};
use super::LARGEST;
use creeer_math::codec::{DecodeError, Reader, Writer};
use creeer_math::glam::{DMat3, DQuat, DVec3};

fn scheme_tag(scheme: Scheme) -> u8 {
    match scheme {
        Scheme::BackwardEuler => 0,
        Scheme::Bdf2 => 1,
        Scheme::Newmark => 2,
    }
}

/// Writes what of a solver lasts from step to step: its settings and its history.
pub(crate) fn write_solver(solver: &ImplicitSolver, w: &mut Writer) {
    w.u8(scheme_tag(solver.scheme));
    w.put(&solver.substeps);
    w.f64(solver.h);
    w.put(&solver.max_iterations);
    w.f64(solver.tolerance);
    w.u8(match solver.refactoring {
        Refactoring::Never => 0,
        Refactoring::WhenSlow => 1,
        Refactoring::EveryIteration => 2,
    });
    w.put(&solver.energy_guard);
    w.put(&solver.fallbacks);
    w.put(&solver.previous_x);
    w.put(&solver.previous_v);
    w.put(&solver.previous_q);
    w.put(&solver.previous_w);
    w.put(&solver.acceleration);
    w.put(&solver.angular_acceleration);
    w.put(&solver.wanted);
    // What its matrix was built from (`read_solver` builds the same one).
    w.put(&solver.factorisations);
    w.put(&solver.factor_current);
    w.put(&solver.factor_position);
    w.put(&solver.factor_orientation);
    w.put(&solver.factor_broken);
    w.varint(solver.contact_blocks.len() as u64);
    for (node, block) in &solver.contact_blocks {
        w.put(node);
        w.put(block);
    }
}

/// Reads a solver written by [`write_solver`] for `structure`.
///
/// # Errors
/// If the bytes are not one, or its history is not of this structure.
pub(crate) fn read_solver(
    structure: &Structure,
    r: &mut Reader<'_>,
) -> Result<ImplicitSolver, DecodeError> {
    let scheme = match r.u8()? {
        0 => Scheme::BackwardEuler,
        1 => Scheme::Bdf2,
        2 => Scheme::Newmark,
        _ => return Err(r.error("unknown scheme")),
    };
    let substeps: u32 = r.get()?;
    let h = r.finite()?;
    if substeps == 0 || h <= 0.0 {
        return Err(r.error("a solver without a step"));
    }
    let mut solver = ImplicitSolver::try_new(structure, scheme, h * f64::from(substeps), substeps)
        .ok_or_else(|| r.error("a structure whose matrix is not positive definite"))?;
    solver.h = h;
    solver.max_iterations = r.get()?;
    solver.tolerance = r.finite()?;
    solver.refactoring = match r.u8()? {
        0 => Refactoring::Never,
        1 => Refactoring::WhenSlow,
        2 => Refactoring::EveryIteration,
        _ => return Err(r.error("unknown refactoring")),
    };
    solver.energy_guard = r.get()?;
    solver.fallbacks = r.get()?;
    solver.previous_x = r.get()?;
    solver.previous_v = r.get()?;
    solver.previous_q = r.get()?;
    solver.previous_w = r.get()?;
    solver.acceleration = r.get()?;
    solver.angular_acceleration = r.get()?;
    solver.wanted = r.get()?;
    let n = structure.node_count();
    let lengths = [
        solver.previous_x.len(),
        solver.previous_v.len(),
        solver.previous_q.len(),
        solver.previous_w.len(),
        solver.acceleration.len(),
        solver.angular_acceleration.len(),
    ];
    if lengths.iter().any(|&m| m != n) {
        return Err(r.error("a solver's history is of another structure"));
    }
    let history = [
        &solver.previous_x,
        &solver.previous_v,
        &solver.previous_w,
        &solver.acceleration,
    ];
    let history_ok = history
        .iter()
        .all(|v| v.iter().all(|x| x.abs().max_element() <= LARGEST))
        && solver
            .angular_acceleration
            .iter()
            .all(|x| x.abs().max_element() <= LARGEST)
        && solver
            .previous_q
            .iter()
            .all(|q| (q.length() - 1.0).abs() < 1e-6)
        && solver.h <= 1.0
        && solver.tolerance > 0.0;
    if !history_ok {
        return Err(r.error("a solver with numbers no world has"));
    }
    if solver
        .wanted
        .iter()
        .any(|&e| e >= structure.element_count())
    {
        return Err(r.error("a solver wants a member the structure lacks"));
    }
    let factorisations: u32 = r.get()?;
    let current: bool = r.get()?;
    let position: Vec<DVec3> = r.get()?;
    let orientation: Vec<DQuat> = r.get()?;
    let broken: Vec<bool> = r.get()?;
    let blocks = (0..r.count(10)?)
        .map(|_| Ok((r.get::<usize>()?, r.get()?)))
        .collect::<Result<Vec<(usize, DMat3)>, DecodeError>>()?;
    if position.len() != n || orientation.len() != n || broken.len() != structure.element_count() {
        return Err(r.error("a solver's matrix is of another structure"));
    }
    let shape_ok = position.iter().all(|x| x.abs().max_element() <= LARGEST)
        && orientation.iter().all(|q| (q.length() - 1.0).abs() < 1e-6)
        && blocks
            .iter()
            .all(|(_, b)| b.to_cols_array().iter().all(|x| x.abs() <= LARGEST));
    if !shape_ok {
        return Err(r.error("a matrix built from numbers no world has"));
    }
    if blocks.iter().any(|(node, _)| *node >= 2 * n) {
        return Err(r.error("a contact block on a node the structure lacks"));
    }
    // The matrix as it was built: at the shape it was built at, with the members broken
    // since still whole in it.
    let mut then = structure.clone();
    then.orientation = orientation;
    if current {
        then.position = position;
    } else if position != structure.rest {
        return Err(r.error("a matrix at a rest shape the structure lacks"));
    }
    for (e, was) in then.elements.iter_mut().zip(&broken) {
        if e.broken && !*was {
            e.stiffness = Stiffness::new(&e.section, &e.material, e.rest_length);
        }
    }
    solver.contact_blocks = blocks;
    if !solver.try_factor(&then, current) {
        return Err(r.error("a matrix that is not positive definite"));
    }
    solver.factor_broken = broken;
    solver.factorisations = factorisations;
    Ok(solver)
}
