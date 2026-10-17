//! Finishing a step: the parts' new speeds, the shafts' twists and the gears' places in
//! their play, what each element carried, the energy, and the torque on the housing.

use crate::drivetrain::{Drivetrain, Element, Grip, HOUSING};
use crate::elements::{self, Flow, Terms};
use crate::step::speed;
use creeer_math::glam::DVec3;
use creeer_network::Sum;

/// The torques an element puts on its nodes at the solution (N·m about their axes).
fn torques(
    d: &Drivetrain,
    k: usize,
    x: &[f64],
    column: &[Option<usize>],
    grip: &[Grip],
    forward: &[Flow],
    h: f64,
) -> Terms {
    let e = &d.elements[k];
    let mut terms = match (e, column[k]) {
        (
            Element::Shaft {
                a,
                b,
                stiffness,
                damping,
            },
            _,
        ) => {
            let rate = speed(x, *a) - speed(x, *b);
            let t = stiffness * (d.twist[k] + h * rate) + damping * rate;
            [(*a, -t), (*b, t), (HOUSING, 0.0)]
        }
        (_, Some(c)) => elements::rigid(e, forward[k])
            .1
            .map(|(node, t)| (node, t * x[c])),
        (_, None) => elements::sliding(e, grip[k], forward[k]),
    };
    if let Element::Bearing { a, viscous, .. } = *e {
        terms[0] = (a, terms[0].1 - viscous * speed(x, a));
    }
    terms
}

/// Sets the drivetrain to the solution `x` of a step of `h`, books its energy and the
/// housing's reaction.
pub(crate) fn finish(
    d: &mut Drivetrain,
    h: f64,
    spin: DVec3,
    x: &[f64],
    column: &[Option<usize>],
    grip: &[Grip],
    forward: &[Flow],
) {
    let mut reaction = DVec3::ZERO;
    let mut dissipated = Sum::default();
    // The speeds midway through the step: J·(ω − ω₀)/h = ΣT times h·(ω + ω₀)/2 is exactly
    // ΔKE = h·Σ T·ω_mid, so every joule is some element's (no remainder of backward Euler's
    // own), and a slipping clutch's heat is the trapezoid's (end speeds miss C·h/(J·Δω) of it:
    // a sixth of a small part's brake heat at 60 Hz).
    let mid = |node: usize| {
        if node == HOUSING {
            0.0
        } else {
            0.5 * (x[node - 1] + d.parts[node].speed)
        }
    };
    for k in 0..d.elements.len() {
        let terms = torques(d, k, x, column, grip, forward, h);
        // The power its torques put into the parts (negative: it took it).
        let power: f64 = terms.iter().map(|&(node, t)| t * mid(node)).sum();
        match d.elements[k] {
            Element::Shaft {
                a, b, stiffness, ..
            } => {
                // What it took less what its strain gained: its damping and backward Euler's
                // (which, measured at the midpoint, can go either way within a cycle by O(h²);
                // not heat for the thermal network).
                let phi = d.twist[k];
                d.twist[k] += h * (speed(x, a) - speed(x, b));
                let strain = 0.5 * stiffness * (d.twist[k] * d.twist[k] - phi * phi);
                dissipated.add(-power * h - strain);
            }
            _ => {
                d.heat[k].add(-power * h);
                dissipated.add(-power * h);
            }
        }
        if let Element::Gear {
            a,
            b,
            ratio,
            backlash,
            ..
        } = d.elements[k]
            && backlash > 0.0
        {
            // Its place in its play: at a flank while meshed, moving within it while free.
            let half = 0.5 * backlash;
            d.twist[k] = match grip[k] {
                Grip::Meshed(f) => f64::from(f) * half,
                _ => (d.twist[k] + h * (speed(x, b) - ratio * speed(x, a))).clamp(-half, half),
            };
        }
        // What it carried from its first node: the opposite of its torque on that node.
        d.carried[k] = -terms[0].1;
        for (node, t) in terms {
            if node != HOUSING {
                reaction -= d.parts[node].axis * t;
            }
        }
    }
    let (mut imported, mut exported) = (Sum::default(), Sum::default());
    for (i, p) in d.parts.iter_mut().enumerate().skip(1) {
        // Each torque's work on its own: a motor's is what its circuit exported.
        let mid = 0.5 * (p.speed + x[i - 1]);
        p.speed = x[i - 1];
        for work in [p.torque * mid * h, p.load * mid * h] {
            if work >= 0.0 {
                imported.add(work)
            } else {
                exported.add(-work)
            }
        }
        // A torque from the housing (a motor's): its stator takes the opposite. A load's
        // reaction is on whatever applied it (the ground under a wheel), not the housing.
        reaction -= p.axis * p.torque;
    }
    // The parts' angular momentum is carried round as the housing turns: the housing turns it.
    reaction -= spin.cross(d.momentum());
    d.reaction = reaction;
    // Each booked on its own: the ledger's closure checks the solve (step.rs's identity).
    d.ledger.import(imported.value());
    d.ledger.export(exported.value());
    d.ledger.dissipate(dissipated.value());
}
