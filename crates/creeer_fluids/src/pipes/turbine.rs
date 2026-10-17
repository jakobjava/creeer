//! Turbines: a link that takes work from the water for a shaft (F6.8, ADR-0016).
//!
//! **Euler's turbomachine equation.** The water enters the runner with the swirl its guide
//! vanes give it and leaves with the blades' own: the torque is ρ·Q·(r₁·c_u1 − r₂·c_u2), and
//! with both swirls in proportion to the flow and the outlet's carried with the blades
//! (c_u2 = ω·r₂ − Q·cot β₂/A₂), T = ρ·Q·(a·Q − r₂²·ω), a = r₁·cot α₁/A₁ + r₂·cot β₂/A₂. The head it
//! takes for that work is T·ω/(ρ·g·Q) = ω·(a·Q − r₂²·ω)/g; friction in its passages takes
//! k·Q|Q| more. Held still (ω = 0) it is a nozzle with the stall torque ρ·a·Q²; let run free,
//! it turns at ω = a·Q/r₂² (runaway), its torque gone.
//!
//! **Its books.** A step dissipates the water's whole drop across the turbine, as it does a
//! pipe's; the work its shaft takes each tick ([`Network::turbine_work`], what the drivetrain
//! books as imported) moves from dissipated to exported. The network's books close at every
//! tick, and the shaft's joules are the drivetrain's, exactly. Between a slow network's steps
//! the shaft's speed moves on from the one its step solved with: the turbine's remaining loss
//! over the interval then carries that difference too.

use super::{LinkKind, Network};

impl Network {
    /// Sets turbine `link`'s runner speed (rad/s) for the steps to come.
    ///
    /// # Panics
    /// If the link is no turbine.
    pub fn set_turbine_speed(&mut self, link: usize, omega: f64) {
        let LinkKind::Turbine { speed, .. } = &mut self.links[link].kind else {
            panic!("link {link} is no turbine");
        };
        *speed = omega;
    }

    /// Turbine `link`'s torque on its shaft (N·m) at the flow its network last found, as a
    /// linear function of the shaft's speed ω: (its torque at rest, its slope by ω), T = T₀ +
    /// dT/dω·ω = ρ·a·Q² − ρ·Q·r²·ω.
    ///
    /// # Panics
    /// If the link is no turbine.
    pub fn turbine_curve(&self, link: usize) -> (f64, f64) {
        let l = &self.links[link];
        let LinkKind::Turbine { swirl, radius, .. } = l.kind else {
            panic!("link {link} is no turbine");
        };
        let (rho, q) = (self.liquid.density, if l.open { l.flow } else { 0.0 });
        (rho * swirl * q * q, -rho * q * radius * radius)
    }

    /// Turbine `link`'s torque on a shaft turning at `omega` (N·m).
    pub fn turbine_torque(&self, link: usize, omega: f64) -> f64 {
        let (at_rest, slope) = self.turbine_curve(link);
        at_rest + slope * omega
    }

    /// `joules` went from turbine `link`'s water into its shaft (the drivetrain's import):
    /// exported, and no longer the turbine's loss. Negative, the shaft drove the water (faster
    /// than runaway): imported, and the step's negative loss across the runner made good.
    pub fn turbine_work(&mut self, link: usize, joules: f64) {
        debug_assert!(matches!(self.links[link].kind, LinkKind::Turbine { .. }));
        if joules >= 0.0 {
            self.ledger.export(joules);
        } else {
            self.ledger.import(-joules);
        }
        self.ledger.dissipate(-joules);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Liquid, NodeKind};
    use super::*;

    const G: f64 = 9.806_65;

    /// A turbine between two reservoirs 20 m apart, its runner held at `omega`.
    fn plant(omega: f64) -> (Network, usize) {
        let mut net = Network::new(Liquid::WATER_20C, G);
        let upper = net.node(20.0, NodeKind::Reservoir, 20.0);
        let lower = net.node(0.0, NodeKind::Reservoir, 0.0);
        let t = net.link(
            upper,
            lower,
            LinkKind::Turbine {
                swirl: 400.0,
                radius: 0.15,
                loss: 2.0e4,
                speed: omega,
            },
        );
        net.tick(1.0 / 60.0);
        (net, t)
    }

    /// Held still, the water runs as through a nozzle (H = k·Q²) and pushes with ρ·a·Q²; let
    /// run at Euler's runaway speed a·Q/r², it passes the same flow and pushes not at all.
    #[test]
    fn a_turbine_stalls_and_runs_away_as_eulers_equation_says() {
        let (net, t) = plant(0.0);
        let q = (20.0 / 2.0e4_f64).sqrt();
        assert!(
            (net.links[t].flow / q - 1.0).abs() < 1e-9,
            "{}",
            net.links[t].flow
        );
        let stall = 998.2 * 400.0 * q * q;
        let torque = net.turbine_torque(t, 0.0);
        assert!(
            (torque / stall - 1.0).abs() < 1e-3,
            "{torque} against {stall}"
        );
        let runaway = 400.0 * q / (0.15 * 0.15);
        let (net, t) = plant(runaway);
        assert!((net.links[t].flow / q - 1.0).abs() < 1e-6);
        assert!(net.turbine_torque(t, runaway).abs() < 1e-6 * stall);
    }

    /// In between, the shaft takes T·ω = ρ·g·Q·(the head it takes), and the books close with
    /// it exported.
    #[test]
    fn a_turbines_shaft_takes_the_water_head_it_drops() {
        let omega = 200.0;
        let (mut net, t) = plant(omega);
        let q = net.links[t].flow;
        let euler_head = omega * (400.0 * q - 0.15 * 0.15 * omega) / G;
        let rho = net.liquid.density;
        let power = net.turbine_torque(t, omega) * omega;
        assert!((power / (rho * G * q * euler_head) - 1.0).abs() < 1e-12);
        net.turbine_work(t, power / 60.0);
        let (_, dissipated, exported, _) = net.ledger().totals();
        assert!((exported - power / 60.0).abs() < 1e-9);
        // What is left as loss is friction's: ρ·g·Q·k·Q².
        let friction = rho * G * q * 2.0e4 * q * q / 60.0;
        assert!(
            (dissipated / friction - 1.0).abs() < 1e-9,
            "{dissipated} against {friction}"
        );
        assert!(net.ledger().relative_imbalance(net.stored()) < 1e-12);
    }
}
