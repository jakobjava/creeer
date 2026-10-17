//! Members at temperature (F6.2): a heated member lengthens by its material's thermal strain
//! (its expansion coefficient integrated from the temperature it was built at), and is as
//! stiff and as strong as its material is at that temperature (EN 1993-1-2's reduction
//! factors for steel, in the materials database). A heated member held at its ends pushes on
//! them; a fire weakens a frame until it sags.

use super::Structure;
use super::element::Stiffness;
use super::section::Material;

/// The temperature members are built at, their rest lengths measured at (K): 20 °C.
pub const BUILT_AT: f64 = 293.15;

impl Structure {
    /// Member `element` (of `material`, as the materials database has it) is at temperature
    /// `t` (K). Returns its axial stiffness over what it was (1 if broken): a solver stepping
    /// the structure should be refactorised once its members have drifted far from what it
    /// was factorised with (`ImplicitSolver::refactor`; it converges without, more slowly).
    ///
    /// # Panics
    /// If there is no such member.
    pub fn set_temperature(
        &mut self,
        element: usize,
        t: f64,
        material: &creeer_materials::Material,
    ) -> f64 {
        let e = &mut self.elements[element];
        e.thermal = e.rest_length * material.expansion.integral(BUILT_AT, t);
        e.material = Material::from(material.structural(t));
        if e.broken {
            return 1.0;
        }
        let before = e.stiffness.axial;
        e.stiffness = Stiffness::new(&e.section, &e.material, e.rest_length);
        e.stiffness.axial / before
    }

    /// Each member's ends, mass (kg), length (m) and section: what a thermal network makes
    /// of it (its heat capacity, the conduction along it, the surface it shows).
    pub fn member_geometry(&self) -> Vec<(usize, usize, f64, f64, super::Section)> {
        self.elements
            .iter()
            .map(|e| (e.a, e.b, e.mass(), e.rest_length, e.section))
            .collect()
    }
}
