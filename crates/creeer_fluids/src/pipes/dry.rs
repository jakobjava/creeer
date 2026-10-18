//! Tanks running dry (F7, found by the powerhouse draining its tank: the level fell below
//! the tank's floor, water from nowhere). As EPANET does: a tank whose water is down to its
//! floor is *dry*, and its links carry water only into it (each a check valve towards it,
//! opening again when the head beyond would fill it). A step in which a tank would run dry
//! is cut where it does: stepped to that moment (found by the secant method), then on, dry,
//! for the rest. No tank holds less than nothing, and the books close as in every step.

use super::{Link, LinkKind, Network, NodeKind, PipeReport};

/// A tank this close above its floor (m, relative to the floor's height, at least 1) is dry.
const DRY: f64 = 1e-9;
/// Secant iterations at most for the moment a tank runs dry.
const SECANT: usize = 8;

/// The direction a link may carry water in this step: `Some(1)` only from `a` to `b` (a
/// check valve, a pump, a link into a dry tank at `b`), `Some(-1)` only from `b` to `a`,
/// `Some(0)` neither (both ways barred), `None` either.
pub(crate) fn allowed(link: &Link, dry: &[bool]) -> Option<f64> {
    let forward = super::links::one_way(link) || dry[link.b];
    let backward = dry[link.a];
    match (forward, backward) {
        (true, true) => Some(0.0),
        (true, false) => Some(1.0),
        (false, true) => Some(-1.0),
        (false, false) => None,
    }
}

/// Opens or shuts `link` (its flow just found, `drive` the heads' difference across it) to
/// the direction it may carry water in; returns whether it switched.
pub(crate) fn switch(link: &mut Link, drive: f64, allowed: Option<f64>) -> bool {
    let Some(sign) = allowed else {
        // Free both ways: open (shut only while a tank it drew from was dry).
        let shut = !link.open;
        link.open = true;
        return shut;
    };
    if link.open && (sign == 0.0 || sign * link.flow < 0.0) {
        (link.open, link.flow) = (false, 0.0);
        return true;
    }
    if !link.open && sign != 0.0 {
        // Shut: would it push the way it may? A pump if its shut-off head beats the lift.
        let push = match link.kind {
            LinkKind::Pump { shutoff, speed, .. } if sign > 0.0 => speed * speed * shutoff + drive,
            _ => sign * drive,
        };
        if push > 0.0 {
            link.open = true;
            return true;
        }
    }
    false
}

impl Network {
    /// Which nodes are dry tanks.
    fn dry(&self) -> Vec<bool> {
        (self.nodes.iter())
            .map(|n| match n.kind {
                NodeKind::Tank { floor, .. } => n.head <= floor + DRY * floor.abs().max(1.0),
                _ => false,
            })
            .collect()
    }

    /// The tank (not dry at the start) furthest below its floor after a step from `old`.
    fn ran_dry(&self, old: &[f64], dry: &[bool]) -> Option<usize> {
        (self.nodes.iter().enumerate())
            .filter(|&(i, n)| {
                !dry[i] && matches!(n.kind, NodeKind::Tank { floor, .. } if n.head < floor)
            })
            .map(|(i, n)| {
                let NodeKind::Tank { floor, .. } = n.kind else {
                    unreachable!("a tank")
                };
                (i, (floor - n.head) / (old[i] - n.head))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Advances the network by `dt` seconds: its flows to the heads now, its tanks filling
    /// or emptying (and running dry: the step cut where one does). Returns how Newton went.
    ///
    /// # Panics
    /// If its system is not positive definite (impossible: every law's slope is positive).
    pub fn step(&mut self, dt: f64) -> PipeReport {
        let dry = self.dry();
        let (nodes, links, ledger) = (self.nodes.clone(), self.links.clone(), self.ledger);
        let report = self.step_once(dt, &dry);
        let old: Vec<f64> = nodes.iter().map(|n| n.head).collect();
        let Some(tank) = self.ran_dry(&old, &dry) else {
            return report;
        };
        let NodeKind::Tank { floor, .. } = self.nodes[tank].kind else {
            unreachable!("a tank")
        };
        // The moment it runs dry: where its level reaches its floor (secant on the fraction
        // of the step, from the full step's level).
        let restore = |net: &mut Self| {
            net.nodes.clone_from(&nodes);
            net.links.clone_from(&links);
            net.ledger = ledger;
        };
        let (mut f0, mut e0) = (0.0, old[tank] - floor);
        let (mut f1, mut e1) = (1.0, self.nodes[tank].head - floor);
        for _ in 0..SECANT {
            let f = (f1 - e1 * (f1 - f0) / (e1 - e0)).clamp(0.0, 1.0);
            restore(self);
            self.step_once(f * dt, &dry);
            let e = self.nodes[tank].head - floor;
            (f0, e0, f1, e1) = (f1, e1, f, e);
            if e.abs() <= DRY * floor.abs().max(1.0) * 0.5 {
                break;
            }
        }
        let rest = (1.0 - f1) * dt;
        let dry = self.dry();
        if rest > 0.0 {
            self.step_once(rest, &dry)
        } else {
            report
        }
    }
}
