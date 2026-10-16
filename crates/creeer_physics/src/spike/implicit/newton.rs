//! The implicit substep as an energy minimisation ("optimisation integrator",
//! Gast et al. 2015).
//!
//! Backward Euler and BDF2 both find the new position as the minimum of
//!
//! G(x) = Σ m/(2β²)·|x − x*|² + E(x),  x* = x̂ + β·v̂ (the predicted position),
//!
//! where E is the elastic plus gravitational energy. The classic linearly implicit step is a
//! single Newton step on G. With the rest-shape matrix, that step can overshoot badly when
//! members rotate a lot within one step (an impact), and the simulation explodes. Here every
//! Newton step must lower G (halving the step until it does), so it cannot explode, and the
//! iterations stop as soon as nodes no longer move.

use super::*;
use creeer_math::glam::DMat3;

/// Most step halvings tried in one line search.
const MAX_HALVINGS: u32 = 12;

impl ImplicitTruss {
    pub(super) fn substep(&mut self) {
        let beta = self.beta();
        let t = &mut self.truss;
        for i in 0..t.node_count() {
            let (x, v) = (t.position[i], t.velocity[i]);
            let (x_hat, v_hat) = match self.scheme {
                Scheme::BackwardEuler => (x, v),
                Scheme::Bdf2 => (
                    (4.0 * x - self.previous_x[i]) / 3.0,
                    (4.0 * v - self.previous_v[i]) / 3.0,
                ),
            };
            self.previous_x[i] = x;
            self.previous_v[i] = v;
            self.x_hat[i] = x_hat;
            self.predicted[i] = x_hat + v_hat * beta;
            // The prediction is the first guess.
            t.position[i] = self.predicted[i];
        }

        let body_frame = self.linearisation == Linearisation::BodyFrame && !self.anchored;
        match self.linearisation {
            Linearisation::Corotational => {
                let predicted = std::mem::take(&mut self.predicted);
                self.refactor(&predicted);
                self.predicted = predicted;
            }
            Linearisation::BodyFrame if body_frame => {
                self.rotation = self.matcher.update(&self.predicted);
            }
            Linearisation::BodyFrame | Linearisation::RestFrame => {}
        }
        let rotation = DMat3::from_quat(self.rotation);

        let mut objective = self.objective_at(None);
        for _ in 0..self.max_iterations {
            self.last_iterations += 1;
            // Newton direction d = (M/β² + K)⁻¹·(−∇G) = β²·(M + β²K)⁻¹·(f − M·(x − x*)/β²).
            let t = &self.truss;
            t.forces(&t.position, &mut self.rhs);
            for i in 0..t.node_count() {
                let inertia = (t.position[i] - self.predicted[i]) * (t.mass[i] / (beta * beta));
                let r = (self.rhs[i] - inertia * t.free_mask(i)) * (beta * beta);
                self.rhs[i] = if body_frame {
                    rotation.transpose() * r
                } else {
                    r
                };
            }
            self.chol.solve(&mut self.rhs);
            if body_frame {
                for d in &mut self.rhs {
                    *d = rotation * *d;
                }
            }
            let longest = self.rhs.iter().map(|d| d.length()).fold(0.0, f64::max);

            let mut alpha = 1.0;
            let mut accepted = None;
            for _ in 0..=MAX_HALVINGS {
                let trial_objective = self.objective_at(Some(alpha));
                if trial_objective <= objective {
                    accepted = Some(trial_objective);
                    break;
                }
                alpha *= 0.5;
            }
            // No decrease even for a tiny step: G is at its minimum to machine precision.
            let Some(value) = accepted else { break };
            objective = value;
            std::mem::swap(&mut self.truss.position, &mut self.trial);
            if alpha * longest < self.tolerance {
                break;
            }
        }

        let t = &mut self.truss;
        for i in 0..t.node_count() {
            t.velocity[i] = (t.position[i] - self.x_hat[i]) / beta;
        }
    }

    /// G at the current positions (`None`), or at the trial point x + α·d (the trial
    /// positions are left in `self.trial`). Terms are taken relative to x* so that the large
    /// absolute gravitational energy does not swamp small differences in rounding.
    fn objective_at(&mut self, step: Option<f64>) -> f64 {
        if let Some(alpha) = step {
            for ((trial, x), d) in self
                .trial
                .iter_mut()
                .zip(&self.truss.position)
                .zip(&self.rhs)
            {
                *trial = *x + *d * alpha;
            }
        }
        let beta = self.beta();
        let t = &self.truss;
        let positions = if step.is_some() {
            &self.trial
        } else {
            &t.position
        };
        let mut value = 0.0;
        for ((x, x_star), m) in positions.iter().zip(&self.predicted).zip(&t.mass) {
            let offset = *x - *x_star;
            value += m * (offset.length_squared() / (2.0 * beta * beta) - GRAVITY.dot(offset));
        }
        for beam in &t.beams {
            let stretch = (positions[beam.a] - positions[beam.b]).length() - beam.rest_length;
            value += 0.5 * stretch * stretch / beam.compliance;
        }
        value
    }
}
