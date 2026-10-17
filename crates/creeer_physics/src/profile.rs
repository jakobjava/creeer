//! Where a step's time goes, for finding what makes a scene slow (feature `profile`).
//!
//! Each [`Slot`] sums nanoseconds (or counts) across threads. Without the feature every call
//! here compiles to nothing, so the solver pays nothing for it.

/// What is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// A world's contact detection, its groups' steps, and the rest (anchors, sleep).
    Detect,
    Pass,
    After,
    /// World steps run (substeps by turn included), and steps redone for their turn.
    WorldSteps,
    TurnRedos,
    /// Impact substeps of groups with structures.
    ImpactSubsteps,
    /// Group steps (substeps), and their Newton iterations (the most of any member).
    GroupSteps,
    Iterations,
    /// The energy guard, prediction, residuals, line searches, solves.
    Guard,
    Predict,
    Residual,
    Objective,
    Solve,
    /// Line-search trials.
    Trials,
    /// Friction (lag) refreshes that changed something, and iterations after the first one.
    LagUpdates,
    LagIterations,
    /// A minimisation's setup (matrices of rigid bodies, joint systems), its rebuilding of
    /// rigid matrices at each iteration (Newton's method proper).
    Setup,
    Relinearise,
    /// Factorisations of structures' matrices, and their time.
    Factorisations,
    Factorise,
    /// Plastic flow after a step.
    Yield,
    /// The pass's own work before its groups step (grouping, building contacts), and a
    /// group's world-level work around its solve (impact laws, buckling, friction anchors).
    PassSetup,
    GroupWorld,
    /// Contacts' energy: their geometry (gaps, slips, measured depths), their normal laws
    /// (patches' included), and the rest (dashpots, friction, twist).
    ContactGeometry,
    ContactLaws,
    ContactRest,
    /// Plastic splits of members, sweeps over their end-moment pairs, rate-lifted
    /// increments, and those increments' Newton iterations.
    Splits,
    Sweeps,
    Increments,
    FlowIterations,
}

/// How many slots there are.
pub const SLOTS: usize = Slot::FlowIterations as usize + 1;

#[cfg(feature = "profile")]
static TOTALS: [std::sync::atomic::AtomicU64; SLOTS] =
    [const { std::sync::atomic::AtomicU64::new(0) }; SLOTS];

/// Adds `amount` (ns or a count) to `slot`.
#[inline(always)]
pub fn add(slot: Slot, amount: u64) {
    #[cfg(feature = "profile")]
    TOTALS[slot as usize].fetch_add(amount, std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(feature = "profile"))]
    let _ = (slot, amount);
}

/// A timer whose time since it started goes to a slot.
#[derive(Debug)]
pub struct Timer {
    #[cfg(feature = "profile")]
    start: std::time::Instant,
}

/// Starts a timer.
#[inline(always)]
pub fn start() -> Timer {
    Timer {
        #[cfg(feature = "profile")]
        start: std::time::Instant::now(),
    }
}

impl Timer {
    /// Adds the time since it started to `slot`.
    #[inline(always)]
    pub fn stop(self, slot: Slot) {
        #[cfg(feature = "profile")]
        add(
            slot,
            u64::try_from(self.start.elapsed().as_nanos()).unwrap_or(u64::MAX),
        );
        #[cfg(not(feature = "profile"))]
        let _ = slot;
    }
}

/// Every slot's total since the last call, cleared (all zero without the feature).
pub fn take() -> [u64; SLOTS] {
    #[cfg(feature = "profile")]
    {
        std::array::from_fn(|k| TOTALS[k].swap(0, std::sync::atomic::Ordering::Relaxed))
    }
    #[cfg(not(feature = "profile"))]
    {
        [0; SLOTS]
    }
}

/// The slots in order, with their names, for a report.
pub const NAMES: [(Slot, &str); SLOTS] = [
    (Slot::Detect, "detect"),
    (Slot::Pass, "pass"),
    (Slot::After, "after"),
    (Slot::WorldSteps, "world steps"),
    (Slot::TurnRedos, "turn redos"),
    (Slot::ImpactSubsteps, "impact substeps"),
    (Slot::GroupSteps, "group steps"),
    (Slot::Iterations, "iterations"),
    (Slot::Guard, "guard"),
    (Slot::Predict, "predict"),
    (Slot::Residual, "residual"),
    (Slot::Objective, "objective"),
    (Slot::Solve, "solve"),
    (Slot::Trials, "trials"),
    (Slot::LagUpdates, "lag updates"),
    (Slot::LagIterations, "lag iterations"),
    (Slot::Setup, "setup"),
    (Slot::Relinearise, "relinearise"),
    (Slot::Factorisations, "factorisations"),
    (Slot::Factorise, "factorise"),
    (Slot::Yield, "yield"),
    (Slot::PassSetup, "pass setup"),
    (Slot::GroupWorld, "group world"),
    (Slot::ContactGeometry, "contact geometry"),
    (Slot::ContactLaws, "contact laws"),
    (Slot::ContactRest, "contact rest"),
    (Slot::Splits, "splits"),
    (Slot::Sweeps, "sweeps"),
    (Slot::Increments, "increments"),
    (Slot::FlowIterations, "flow iterations"),
];

/// Whether a slot holds a count (else nanoseconds).
pub fn is_count(slot: Slot) -> bool {
    matches!(
        slot,
        Slot::WorldSteps
            | Slot::TurnRedos
            | Slot::ImpactSubsteps
            | Slot::GroupSteps
            | Slot::Iterations
            | Slot::Trials
            | Slot::LagUpdates
            | Slot::LagIterations
            | Slot::Factorisations
            | Slot::Splits
            | Slot::Sweeps
            | Slot::Increments
            | Slot::FlowIterations
    )
}
