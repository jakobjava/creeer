//! A tree grown from its species and a seed (`docs/adr/0031-trees.md`, decision 2). In the
//! tree's frame: its base at the origin, its stem up +z.
//!
//! - **The stem** tapers from its diameter at breast height (1.3 m) to its top as
//!   r(z) = r₁.₃·((H − z)/(H − 1.3))^taper (taper ½ a paraboloid, 1 a cone); its height H from
//!   the diameter by the species' height curve.
//! - **Branches** leave from nodes along the live crown, `spacing` apart: a whorl of several, or
//!   one (alternate, a golden angle on from the last). Their bases share what the stem loses in
//!   section across the node (Leonardo's rule: the area of a stem below a fork is the sum of the
//!   areas above it; Eloy 2011), and their reach follows the crown's outline.
//! - **Mass**: the branches' and foliage's fresh biomass from the diameter (the species' biomass
//!   functions), shared among the branches: wood by each branch's section times its length,
//!   foliage by its section (the pipe model: foliage in proportion to the sapwood that feeds it,
//!   Shinozaki et al. 1964).

use super::species::Species;
use crate::scatter::Rng;
use creeer_math::det;
use creeer_math::glam::{DMat3, DVec3};

/// The golden angle (rad): alternate branches turn by it from node to node.
const GOLDEN: f64 = 2.399_963_229_728_653;
/// Slices a stem span is integrated over.
const SLICES: usize = 48;

/// A branch, leaving its node's height on the stem.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Branch {
    /// Its direction about the stem (rad, from the tree's +x towards +y) and from the stem's up
    /// (rad).
    pub azimuth: f64,
    pub angle: f64,
    /// Its length and its radius at the base (m).
    pub length: f64,
    pub radius: f64,
    /// Its fresh wood and foliage (kg).
    pub wood: f64,
    pub foliage: f64,
}

impl Branch {
    /// Its unit direction in the tree's frame.
    pub fn direction(&self) -> DVec3 {
        let (sa, ca) = det::sin_cos(self.angle);
        let (sz, cz) = det::sin_cos(self.azimuth);
        DVec3::new(sa * cz, sa * sz, ca)
    }

    /// Its mass (kg).
    pub fn mass(&self) -> f64 {
        self.wood + self.foliage
    }
}

/// Where branches leave the stem.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Height above the base (m).
    pub z: f64,
    pub branches: Vec<Branch>,
}

/// A grown tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    /// Its diameter at breast height and its height (m).
    pub dbh: f64,
    pub height: f64,
    /// Where its live crown begins (m above the base).
    pub crown_base: f64,
    /// Its stem's taper, and its wood's density (kg/m³).
    pub taper: f64,
    pub density: f64,
    pub nodes: Vec<Node>,
}

impl Tree {
    /// A tree of `species` and diameter `dbh` (m), its branches from `seed`.
    pub fn grow(species: &Species, seed: u64, dbh: f64) -> Self {
        let mut rng = Rng::new(seed);
        let height = species.height_at(dbh);
        let crown_base = height * (1.0 - species.crown.ratio);
        let mut tree = Self {
            dbh,
            height,
            crown_base,
            taper: species.taper,
            density: species.density,
            nodes: Vec::new(),
        };
        let b = &species.branching;
        let widest = species.crown.radius.0 + species.crown.radius.1 * dbh;
        let mut turn = rng.unit() * std::f64::consts::TAU;
        let mut z = crown_base + b.spacing * (0.25 + 0.5 * rng.unit());
        while z < height - 0.5 * b.spacing {
            let spread = b.per_node.1 - b.per_node.0;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few
            let n = b.per_node.0 + ((rng.unit() * f64::from(spread + 1)) as u8).min(spread);
            // What the stem loses in section across the node (Leonardo's rule).
            let (below, above) = (z - 0.5 * b.spacing, (z + 0.5 * b.spacing).min(height));
            let shared = std::f64::consts::PI * {
                let (rb, ra) = (tree.radius(below), tree.radius(above));
                rb * rb - ra * ra
            }
            .max(0.0);
            let weights: Vec<f64> = (0..n).map(|_| 0.7 + 0.6 * rng.unit()).collect();
            let sum: f64 = weights.iter().sum();
            let u = (height - z) / (height - crown_base);
            let reach = widest * species.crown.outline.reach(u);
            let mut branches = Vec::new();
            for (k, w) in weights.iter().enumerate() {
                let azimuth = if n == 1 {
                    turn
                } else {
                    turn + std::f64::consts::TAU
                        * (f64::from(u8::try_from(k).unwrap_or(0)) + 0.3 * (rng.unit() - 0.5))
                        / f64::from(n)
                };
                let angle = (b.angle.0 + rng.unit() * (b.angle.1 - b.angle.0)).to_radians();
                let length = (reach * (0.85 + 0.3 * rng.unit()) / det::sin(angle)).max(0.1);
                branches.push(Branch {
                    azimuth,
                    angle,
                    length,
                    radius: (shared * w / sum / std::f64::consts::PI).sqrt(),
                    wood: 0.0,
                    foliage: 0.0,
                });
            }
            turn += if n == 1 {
                GOLDEN
            } else {
                rng.unit() * std::f64::consts::TAU
            };
            tree.nodes.push(Node { z, branches });
            z += b.spacing * (0.8 + 0.4 * rng.unit());
        }
        // The crown's fresh biomass, shared out.
        let d = dbh * 100.0;
        let wood = species.branch_mass.of(d, height) * species.fresh;
        let foliage = species.foliage_mass.of(d, height) * species.fresh;
        let all = tree.nodes.iter().flat_map(|n| n.branches.iter());
        let (by_volume, by_area) = all.fold((0.0, 0.0), |(v, a), b| {
            let area = b.radius * b.radius;
            (v + area * b.length, a + area)
        });
        for b in tree.nodes.iter_mut().flat_map(|n| n.branches.iter_mut()) {
            let area = b.radius * b.radius;
            b.wood = wood * area * b.length / by_volume.max(f64::MIN_POSITIVE);
            b.foliage = foliage * area / by_area.max(f64::MIN_POSITIVE);
        }
        tree
    }

    /// Only its stem: no branches (what a trunk's boxes need, `felled.rs`).
    pub fn stem_only(species: &Species, dbh: f64) -> Self {
        let height = species.height_at(dbh);
        Self {
            dbh,
            height,
            crown_base: height * (1.0 - species.crown.ratio),
            taper: species.taper,
            density: species.density,
            nodes: Vec::new(),
        }
    }

    /// The stem's radius at height `z` (m; none above the top).
    pub fn radius(&self, z: f64) -> f64 {
        let r = 0.5 * self.dbh;
        if z >= self.height {
            return 0.0;
        }
        r * det::pow((self.height - z) / (self.height - 1.3), self.taper)
    }

    /// The stem's mass between heights `from` and `to` (kg), its centre (tree frame) and its
    /// inertia about that centre (kg·m², tree frame axes).
    pub fn stem(&self, from: f64, to: f64) -> (f64, DVec3, DMat3) {
        let to = to.min(self.height);
        let (mass, centre) = self.stem_exact(from, to);
        // The inertia from slices (discs), about the exact centre.
        let mut lumps = Vec::with_capacity(SLICES);
        #[allow(clippy::cast_precision_loss)]
        let dz = (to - from) / SLICES as f64;
        for k in 0..SLICES {
            #[allow(clippy::cast_precision_loss)]
            let z = from + (k as f64 + 0.5) * dz;
            let r = self.radius(z);
            let m = self.density * std::f64::consts::PI * r * r * dz;
            // A disc's own: ½·m·r² about its axis, ¼·m·r² + m·dz²/12 across.
            let across = m * (0.25 * r * r + dz * dz / 12.0);
            let own = DMat3::from_diagonal(DVec3::new(across, across, 0.5 * m * r * r));
            lumps.push((m, DVec3::new(0.0, 0.0, z), own));
        }
        let (sliced, at, inertia) = combine(&lumps);
        // Moved from the slices' centre to the exact one, scaled to the exact mass.
        let r = DVec3::new(0.0, 0.0, at.z - centre.z);
        let shift = (DMat3::IDENTITY * r.dot(r) - outer(r, r)) * sliced;
        let inertia = (inertia + shift) * (mass / sliced.max(f64::MIN_POSITIVE));
        (mass, centre, inertia)
    }

    /// The stem's mass between `from` and `to` and its centre, exactly: with u = H − z,
    /// r² = r₁.₃²·(u/(H − 1.3))^(2·taper), so ∫u^k·r² dz is a power of u (so the masses of
    /// spans add up, to rounding: a cut's chips and its two pieces are the whole).
    pub fn stem_exact(&self, from: f64, to: f64) -> (f64, DVec3) {
        let (h, p2) = (self.height, 2.0 * self.taper);
        let r = 0.5 * self.dbh;
        let scale = self.density * std::f64::consts::PI * r * r / det::pow(h - 1.3, p2);
        let (ua, ub) = ((h - from).max(0.0), (h - to).max(0.0));
        let power = |k: f64| (det::pow(ua, p2 + k) - det::pow(ub, p2 + k)) / (p2 + k);
        let mass = scale * power(1.0);
        if mass <= 0.0 {
            return (0.0, DVec3::new(0.0, 0.0, from));
        }
        // ∫ z·r² dz = ∫ (H − u)·r² du.
        let moment = scale * (h * power(1.0) - power(2.0));
        (mass, DVec3::new(0.0, 0.0, moment / mass))
    }

    /// Branch `b` of node `node`'s mass at its centre (tree frame) and its inertia there: its
    /// wood a cone's (its centre a quarter of the way out), its foliage spread along it.
    pub fn branch(&self, node: &Node, b: &Branch) -> (f64, DVec3, DMat3) {
        let d = b.direction();
        let base = DVec3::new(0.0, 0.0, node.z);
        let at = |share: f64| base + d * (b.length * share);
        let rod = |m: f64, l: f64| (DMat3::IDENTITY - outer(d, d)) * (m * l * l / 12.0);
        combine(&[
            (b.wood, at(0.25), rod(b.wood, 0.5 * b.length)),
            (b.foliage, at(0.5), rod(b.foliage, b.length)),
        ])
    }

    /// The mass above height `z` (kg) and its centre (tree frame): what a cut there carries.
    pub fn above(&self, z: f64) -> (f64, DVec3) {
        let (m, c, _) = self.stem(z, self.height);
        let mut lumps = vec![(m, c, DMat3::ZERO)];
        for node in self.nodes.iter().filter(|n| n.z > z) {
            for b in &node.branches {
                let (m, c, _) = self.branch(node, b);
                lumps.push((m, c, DMat3::ZERO));
            }
        }
        let (m, c, _) = combine(&lumps);
        (m, c)
    }

    /// How many branches it has.
    pub fn branch_count(&self) -> usize {
        self.nodes.iter().map(|n| n.branches.len()).sum()
    }
}

/// x·yᵀ.
pub(crate) fn outer(x: DVec3, y: DVec3) -> DMat3 {
    DMat3::from_cols(x * y.x, x * y.y, x * y.z)
}

/// Lumps (mass, centre, own inertia about it) as one: its mass, centre and inertia about that
/// (the parallel axis theorem).
pub fn combine(lumps: &[(f64, DVec3, DMat3)]) -> (f64, DVec3, DMat3) {
    let mass: f64 = lumps.iter().map(|l| l.0).sum();
    if mass <= 0.0 {
        return (0.0, DVec3::ZERO, DMat3::ZERO);
    }
    let centre = lumps.iter().map(|l| l.1 * l.0).sum::<DVec3>() / mass;
    let mut inertia = DMat3::ZERO;
    for &(m, c, own) in lumps {
        let r = c - centre;
        inertia += own + (DMat3::IDENTITY * r.dot(r) - outer(r, r)) * m;
    }
    (mass, centre, inertia)
}
