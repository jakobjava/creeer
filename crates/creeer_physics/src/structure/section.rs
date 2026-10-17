//! Materials and cross-sections of structural members.
//!
//! These are the physical inputs of a beam: everything the solver needs (stiffness, mass,
//! stresses) follows from them. Later the materials database (`creeer_materials`) produces
//! [`Material`]s from composition and processing; for now there are reference values.

/// Elastic and strength properties of a member's material (SI units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Young's modulus E (Pa).
    pub youngs_modulus: f64,
    /// Shear modulus G (Pa).
    pub shear_modulus: f64,
    /// Density ρ (kg/m³).
    pub density: f64,
    /// Stress at which plastic (permanent) deformation starts (Pa).
    pub yield_strength: f64,
    /// Stress at which the material breaks (Pa).
    pub ultimate_strength: f64,
    /// Stiffness-proportional damping coefficient c (s): a vibration of angular frequency ω
    /// gets damping ratio ζ = c·ω/2. Represents the energy lost in joints and connections;
    /// the material itself dissipates far less.
    pub damping: f64,
    /// Plastic strain at which the material breaks (elongation at break).
    pub elongation: f64,
    /// Plastic strain at which strain hardening starts: the yield stress holds over a plateau
    /// before it (mild steel's Lüders bands; 0 for metals that harden at once, ∞ for none).
    pub hardening_start: f64,
    /// Plastic strain at which the ultimate strength is reached (uniform elongation); the
    /// stress rises linearly to it from the plateau and holds there until the material breaks.
    pub uniform_elongation: f64,
    /// How the flow stress rises with the strain rate.
    pub rate: RateSensitivity,
}

/// Cowper–Symonds rate sensitivity: at plastic strain rate ε̇ the flow stress is the static
/// one times 1 + (ε̇/D)^{1/q} (Jones, *Structural Impact*, 2012, §8.3). The static stress
/// is taken at `reference`, the rate of the tensile test that measured it: Cowper–Symonds
/// alone adds 8 % for mild steel already there, and a building settling for years would
/// carry that too. Below the reference the stress is the static one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RateSensitivity {
    /// D (1/s); infinite: rate-insensitive.
    pub d: f64,
    /// q.
    pub q: f64,
    /// The strain rate of the static strengths (1/s).
    pub reference: f64,
}

impl RateSensitivity {
    /// No rate sensitivity.
    pub const NONE: Self = Self {
        d: f64::INFINITY,
        q: 1.0,
        reference: 0.0,
    };

    /// Whether the flow stress depends on the rate at all.
    pub fn matters(&self) -> bool {
        self.d.is_finite()
    }
}

impl Material {
    /// The static flow stress after plastic strain `strain` (Pa): the yield strength over the
    /// plateau, rising linearly to the ultimate strength at the uniform elongation.
    pub fn flow_stress(&self, strain: f64) -> f64 {
        let span = self.uniform_elongation - self.hardening_start;
        let rise = self.ultimate_strength - self.yield_strength;
        if strain <= self.hardening_start || span <= 0.0 || !rise.is_finite() || rise <= 0.0 {
            return self.yield_strength;
        }
        self.yield_strength + rise * ((strain - self.hardening_start) / span).min(1.0)
    }

    /// dσ/dε of the static flow stress after plastic strain `strain` (Pa).
    pub fn hardening_modulus(&self, strain: f64) -> f64 {
        let span = self.uniform_elongation - self.hardening_start;
        let rise = self.ultimate_strength - self.yield_strength;
        if strain < self.hardening_start
            || strain >= self.uniform_elongation
            || span <= 0.0
            || !rise.is_finite()
            || rise <= 0.0
        {
            return 0.0;
        }
        rise / span
    }

    /// The same material without hardening or rate sensitivity: perfectly plastic.
    pub fn perfectly_plastic(self) -> Self {
        Self {
            hardening_start: f64::INFINITY,
            rate: RateSensitivity::NONE,
            ..self
        }
    }
}

/// Structural steel S235 (EN 10025): E = 210 GPa, ν = 0.3. Damping gives ζ ≈ 0.6 % at 2 Hz,
/// typical of welded steel frames (0.5–2 %).
/// Past yield: a Lüders plateau to 1.5 % plastic strain, hardening to the ultimate strength at
/// 18 % (uniform elongation), breaking at 26 %; Cowper–Symonds D = 40.4 s⁻¹, q = 5 for mild
/// steel (Jones, *Structural Impact*), referred to the tensile test's 2.5·10⁻⁴ s⁻¹
/// (EN ISO 6892-1, method A1).
pub const STEEL_S235: Material = Material {
    youngs_modulus: 210.0e9,
    shear_modulus: 80.8e9,
    density: 7850.0,
    yield_strength: 235.0e6,
    ultimate_strength: 360.0e6,
    damping: 1.0e-3,
    elongation: 0.26,
    hardening_start: 0.015,
    uniform_elongation: 0.18,
    rate: RateSensitivity {
        d: 40.4,
        q: 5.0,
        reference: 2.5e-4,
    },
};

/// Aluminium alloy 6061-T6: E = 68.9 GPa, ν = 0.33.
/// Past yield it hardens at once, to its ultimate strength at 8 % (uniform elongation), and
/// breaks at 12 %. Taken as rate-insensitive: tests often find 6061-T6 nearly so, while the
/// Cowper–Symonds fit commonly quoted for aluminium alloys (D = 6500 s⁻¹, q = 4) would add a
/// third at crash rates; the materials database is to settle it.
pub const ALUMINIUM_6061_T6: Material = Material {
    youngs_modulus: 68.9e9,
    shear_modulus: 26.0e9,
    density: 2700.0,
    yield_strength: 276.0e6,
    ultimate_strength: 310.0e6,
    damping: 1.0e-3,
    elongation: 0.12,
    hardening_start: 0.0,
    uniform_elongation: 0.08,
    rate: RateSensitivity::NONE,
};

/// Geometric properties of a member's cross-section (SI units). Local axes: x along the
/// member, y and z across it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    /// Area A (m²).
    pub area: f64,
    /// Second moment of area about local y, I_y (m⁴): resists bending in the x-z plane.
    pub i_y: f64,
    /// Second moment of area about local z, I_z (m⁴): resists bending in the x-y plane.
    pub i_z: f64,
    /// Torsion constant J (m⁴). Equals the polar moment only for round sections.
    pub torsion: f64,
    /// Effective shear area as a fraction of A (Timoshenko's κ), along y and along z.
    /// Short, deep members deflect noticeably in shear; slender ones do not.
    pub shear_factor: [f64; 2],
    /// Largest distance from the centroid along y and z (m), for bending stress M·c/I.
    pub extent: [f64; 2],
    /// Plastic section moduli Z about y and z (m³): fully plastic moment M_p = σ_y·Z.
    pub plastic_modulus: [f64; 2],
    /// Plastic torsion modulus (m³): fully plastic torque T_p = τ_y·this, τ_y = σ_y/√3.
    pub plastic_torsion: f64,
    /// Its outline's length (m): the surface it shows to air or fire, per metre of member
    /// (EN 1993-1-2's A_m, the section factor's numerator).
    pub perimeter: f64,
}

impl Section {
    /// Solid round bar of diameter `d`.
    pub fn solid_round(d: f64) -> Self {
        let r = d / 2.0;
        let i = std::f64::consts::PI * r * r * r * r / 4.0;
        Self {
            area: std::f64::consts::PI * r * r,
            i_y: i,
            i_z: i,
            torsion: 2.0 * i,
            shear_factor: [0.9; 2],
            extent: [r; 2],
            plastic_modulus: [d * d * d / 6.0; 2],
            plastic_torsion: std::f64::consts::PI * d * d * d / 12.0,
            perimeter: std::f64::consts::PI * d,
        }
    }

    /// Round tube with outer diameter `d` and wall thickness `t`.
    pub fn round_tube(d: f64, t: f64) -> Self {
        let (ro, ri) = (d / 2.0, d / 2.0 - t);
        let quartic = ro * ro * ro * ro - ri * ri * ri * ri;
        let i = std::f64::consts::PI * quartic / 4.0;
        Self {
            area: std::f64::consts::PI * (ro * ro - ri * ri),
            i_y: i,
            i_z: i,
            torsion: 2.0 * i,
            shear_factor: [0.5; 2],
            extent: [ro; 2],
            plastic_modulus: [(cube(d) - cube(d - 2.0 * t)) / 6.0; 2],
            plastic_torsion: std::f64::consts::PI * (cube(d) - cube(d - 2.0 * t)) / 12.0,
            perimeter: std::f64::consts::PI * d,
        }
    }

    /// Solid rectangle `width` (along y) × `height` (along z).
    pub fn rectangle(width: f64, height: f64) -> Self {
        let (long, short) = if width >= height {
            (width, height)
        } else {
            (height, width)
        };
        // Roark's approximation of the torsion constant of a solid rectangle.
        let ratio = short / long;
        let torsion = long
            * short
            * short
            * short
            * (1.0 / 3.0 - 0.21 * ratio * (1.0 - ratio * ratio * ratio * ratio / 12.0));
        Self {
            area: width * height,
            i_y: width * height * height * height / 12.0,
            i_z: height * width * width * width / 12.0,
            torsion,
            shear_factor: [5.0 / 6.0; 2],
            extent: [width / 2.0, height / 2.0],
            plastic_modulus: [width * height * height / 4.0, height * width * width / 4.0],
            // Sand-heap analogy: a²·(3c − a)/6 for sides a ≤ c.
            plastic_torsion: short * short * (3.0 * long - short) / 6.0,
            perimeter: 2.0 * (width + height),
        }
    }

    /// Doubly symmetric I-beam (web along z): total height `h`, flange width `b`, flange
    /// thickness `tf`, web thickness `tw`. Torsion constant of thin-walled open sections:
    /// J ≈ Σ b·t³/3.
    pub fn i_beam(h: f64, b: f64, tf: f64, tw: f64) -> Self {
        let web = h - 2.0 * tf;
        let cube = |v: f64| v * v * v;
        Self {
            area: 2.0 * b * tf + web * tw,
            i_y: (b * cube(h) - (b - tw) * cube(web)) / 12.0,
            i_z: (2.0 * tf * cube(b) + web * cube(tw)) / 12.0,
            torsion: (2.0 * b * cube(tf) + web * cube(tw)) / 3.0,
            // Shear along z is carried by the web, along y by the flanges.
            shear_factor: [
                2.0 * b * tf / (2.0 * b * tf + web * tw),
                web * tw / (2.0 * b * tf + web * tw),
            ],
            extent: [b / 2.0, h / 2.0],
            plastic_modulus: [
                b * tf * (h - tf) + tw * web * web / 4.0,
                tf * b * b / 2.0 + web * tw * tw / 4.0,
            ],
            plastic_torsion: (2.0 * tf * tf * (3.0 * b - tf) + tw * tw * (3.0 * web - tw)) / 6.0,
            // Its outline: the flanges' faces and edges, and the web's two faces.
            perimeter: 2.0 * h + 4.0 * b - 2.0 * tw,
        }
    }
}

fn cube(v: f64) -> f64 {
    v * v * v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a / b - 1.0).abs() < tolerance
    }

    #[test]
    fn tube_matches_the_difference_of_two_bars() {
        let (outer, inner) = (Section::solid_round(0.2), Section::solid_round(0.16));
        let tube = Section::round_tube(0.2, 0.02);
        assert!(close(tube.area, outer.area - inner.area, 1e-12));
        assert!(close(tube.i_y, outer.i_y - inner.i_y, 1e-12));
    }

    #[test]
    fn square_torsion_constant_matches_the_handbook() {
        // Exact value for a square: J = 0.1406·a⁴ (Roark's formula gives 0.1408).
        let s = Section::rectangle(0.1, 0.1);
        assert!(close(s.torsion, 0.1406 * 1e-4, 0.002), "{}", s.torsion);
    }

    /// HEA 200 from the steel tables: A = 53.8 cm², I_y = 3692 cm⁴, I_z = 1336 cm⁴,
    /// J = 21 cm⁴. The tables include the root fillets this simple shape leaves out: it
    /// comes out 5 % lower in A and I_y and 29 % lower in J (fillets stiffen torsion most).
    #[test]
    fn i_beam_is_close_to_a_rolled_hea_200() {
        let s = Section::i_beam(0.19, 0.2, 0.01, 0.0065);
        assert!(close(s.area, 53.8e-4, 0.06), "A {}", s.area);
        assert!(close(s.i_y, 3692e-8, 0.06), "Iy {}", s.i_y);
        assert!(close(s.i_z, 1336e-8, 0.01), "Iz {}", s.i_z);
        assert!(close(s.torsion, 21e-8, 0.3), "J {}", s.torsion);
    }
}

impl From<creeer_materials::Structural> for Material {
    /// The structure solver's view of an engineering material at one temperature
    /// (`creeer_materials`).
    fn from(s: creeer_materials::Structural) -> Self {
        Self {
            youngs_modulus: s.youngs_modulus,
            shear_modulus: s.shear_modulus,
            density: s.density,
            yield_strength: s.yield_strength,
            ultimate_strength: s.ultimate_strength,
            damping: s.damping,
            elongation: s.elongation,
            hardening_start: s.hardening_start,
            uniform_elongation: s.uniform_elongation,
            rate: s.rate.map_or(RateSensitivity::NONE, |r| RateSensitivity {
                d: r.d,
                q: r.q,
                reference: r.reference,
            }),
        }
    }
}

#[cfg(test)]
mod database_tests {
    use super::*;

    /// The built-in materials are the materials database's, at room temperature.
    #[test]
    fn the_built_in_materials_are_the_database_ones() {
        let table = creeer_materials::MaterialTable::reference();
        let at = |id: &str| Material::from(table.get(id).expect("shipped").structural(293.15));
        assert_eq!(at("steel_s235"), STEEL_S235);
        assert_eq!(at("aluminium_6061_t6"), ALUMINIUM_6061_T6);
    }
}
