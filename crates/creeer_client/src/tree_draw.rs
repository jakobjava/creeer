//! How trees are drawn (G1.7, ADR-0031; G2.11, ADR-0037; `tree_view.rs` gathers them): the stem
//! and branches where the tree's members are, as logs with their species' bark and cut ends,
//! tapering as the stem does (`part_view.rs`); the foliage as masses where the crown is: a
//! conifer's (a cone's crown) in tiers drooping from its whorls, others' in lumpy clumps. Finer
//! the nearer: branch by branch, then a crown of a few tiers or clumps, then one mass.

use crate::part_view::{Form, Made, Thing};
use crate::physics_sandbox::PhysicsSnapshot;
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3};
use creeer_render::{Instance, Material, Scene};
use creeer_sim::trees::species::Outline;
use creeer_sim::trees::{Part, Species, Tree};

/// A mass of foliage's shape (`creeer_render::foliage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mass {
    /// A lumpy clump (a broadleaf's, a pine's).
    Clump,
    /// A conifer's whorl drooping to its tips.
    Tier,
    /// A conifer's whole crown of tiers, far off.
    Spire,
}

/// A mass of foliage to draw: its centre, its turn (its Y up the stem), its half extents, its
/// shape and its species.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foliage {
    pub centre: DVec3,
    pub turn: DQuat,
    pub half: DVec3,
    pub mass: Mass,
    pub species: u16,
}

/// How a tree is placed: tree frame to world, the tree's turn (its frame's +z up the stem), its
/// species' index and a key that makes its parts their own.
#[derive(Clone, Copy)]
pub(crate) struct Placing<F: Fn(DVec3) -> DVec3> {
    pub place: F,
    pub turn: DQuat,
    pub species: u16,
    pub key: u64,
}

/// Turns a mesh's +Y onto the tree frame's +z.
fn up_the_stem(turn: DQuat) -> DQuat {
    turn * DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2)
}

/// A tapering log from `a` to `b` (tree frame), `ra` and `rb` its radii there.
fn log<F: Fn(DVec3) -> DVec3>(
    out: &mut Vec<Thing>,
    at: &Placing<F>,
    (a, b): (DVec3, DVec3),
    (ra, rb): (f64, f64),
    key: u64,
) {
    let (wa, wb) = ((at.place)(a), (at.place)(b));
    let along = wb - wa;
    let length = along.length();
    if length < 1e-6 || ra <= 0.0 {
        return;
    }
    out.push(Thing {
        centre: 0.5 * (wa + wb),
        turn: DQuat::from_rotation_arc(DVec3::X, along / length),
        form: Form::Rod {
            radius: ra,
            length,
            taper: (rb / ra).clamp(0.0, 1.0),
        },
        made: Made::Log(at.species),
        key,
        age: (0.0, 0.0),
    });
}

/// The stem of `tree` between heights `span` as `n` tapering logs.
pub(crate) fn stem<F: Fn(DVec3) -> DVec3>(
    out: &mut PhysicsSnapshot,
    tree: &Tree,
    (from, to): (f64, f64),
    at: &Placing<F>,
    n: usize,
) {
    #[allow(clippy::cast_precision_loss)]
    let step = (to - from) / n as f64;
    for k in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let (a, b) = (from + step * k as f64, from + step * (k + 1) as f64);
        let z = |h: f64| DVec3::new(0.0, 0.0, h);
        let radii = (tree.radius(a).max(0.004), tree.radius(b));
        log(&mut out.things, at, (z(a), z(b)), radii, at.key * 64 + k as u64);
    }
}

/// Whether `species`' foliage is drawn in tiers (a cone's crown: a conifer's whorls) or clumps.
fn tiered(species: &Species) -> bool {
    species.crown.outline == Outline::Cone
}

/// `part`'s branches of `tree`, each a tapering branch, and their foliage: a tier about the stem
/// at each whorl (a conifer), or a clump along each branch.
pub(crate) fn branches<F: Fn(DVec3) -> DVec3>(
    out: &mut PhysicsSnapshot,
    tree: &Tree,
    part: &Part,
    at: &Placing<F>,
    species: &Species,
) {
    let kept = tree.kept(part);
    let tiers = tiered(species);
    for (j, &(k, b)) in kept.iter().enumerate() {
        let node = &tree.nodes[k];
        let branch = &node.branches[b];
        let d = branch.direction();
        let base = DVec3::new(0.0, 0.0, node.z);
        let key = at.key * 4096 + 64 + j as u64;
        log(&mut out.things, at, (base, base + d * branch.length), (branch.radius, 0.2 * branch.radius), key);
        if tiers {
            continue;
        }
        let l = branch.length;
        let centre = (at.place)(base + d * (0.62 * l));
        out.foliage.push(Foliage {
            centre,
            // Its x along the branch's way about the stem.
            turn: up_the_stem(at.turn) * DQuat::from_rotation_y(branch.azimuth),
            half: DVec3::new(0.42 * l, 0.3 * l, 0.36 * l).max(DVec3::splat(0.12)),
            mass: Mass::Clump,
            species: at.species,
        });
    }
    if !tiers {
        return;
    }
    let mut whorls: Vec<usize> = kept.iter().map(|kb| kb.0).collect();
    whorls.dedup();
    for k in whorls {
        let node = &tree.nodes[k];
        let reach = (kept.iter().filter(|kb| kb.0 == k))
            .map(|kb| {
                let b = &node.branches[kb.1];
                creeer_math::det::sin(b.angle) * b.length
            })
            .fold(0.0, f64::max);
        let drop = 0.9 * species.branching.spacing + 0.25 * reach;
        out.foliage.push(Foliage {
            centre: (at.place)(DVec3::new(0.0, 0.0, node.z - 0.35 * drop)),
            turn: up_the_stem(at.turn),
            half: DVec3::new(reach.max(0.2), 0.5 * drop, reach.max(0.2)),
            mass: Mass::Tier,
            species: at.species,
        });
    }
}

/// `stem`'s crown as `masses` masses over its outline: a conifer's as tiers up it (one: a whole
/// spire), others' as clumps about it (one: a single mass).
pub(crate) fn crown<F: Fn(DVec3) -> DVec3>(
    out: &mut PhysicsSnapshot,
    stem: &Tree,
    species: &Species,
    at: &Placing<F>,
    masses: usize,
) {
    let length = stem.height - stem.crown_base;
    let widest = species.crown.radius.0 + species.crown.radius.1 * stem.dbh;
    let up = up_the_stem(at.turn);
    let mut push = |z: f64, out_by: DVec3, half: DVec3, mass: Mass| {
        out.foliage.push(Foliage {
            centre: (at.place)(DVec3::new(0.0, 0.0, z) + out_by),
            turn: up,
            half,
            mass,
            species: at.species,
        });
    };
    if masses <= 1 {
        let mid = stem.crown_base + 0.5 * length;
        let reach = if tiered(species) { widest } else { 0.8 * widest };
        let mass = if tiered(species) { Mass::Spire } else { Mass::Clump };
        push(mid, DVec3::ZERO, DVec3::new(reach, 0.5 * length, reach), mass);
        return;
    }
    // A conifer's tiers up the stem; others' clumps twice as many, round the stem (each turned
    // a golden angle on from the one below), about a clump at its middle that fills it.
    let tiers = tiered(species);
    let count = if tiers { masses } else { 2 * masses };
    #[allow(clippy::cast_precision_loss)]
    let n = count as f64;
    if !tiers {
        let mid = stem.crown_base + 0.5 * length;
        let size = 0.55 * widest;
        push(mid, DVec3::ZERO, DVec3::new(size, 0.4 * length, size), Mass::Clump);
    }
    for k in 0..count {
        #[allow(clippy::cast_precision_loss)]
        let u = (k as f64 + 0.5) / n;
        let z = stem.crown_base + length * u;
        let reach = widest * species.crown.outline.reach((stem.height - z) / length);
        if tiers {
            let thick = 1.7 * length / n;
            let half = DVec3::new(reach.max(0.2), 0.5 * thick, reach.max(0.2));
            push(z - 0.2 * thick, DVec3::ZERO, half, Mass::Tier);
        } else {
            #[allow(clippy::cast_precision_loss)]
            let a = k as f64 * 2.4 + (at.key % 7) as f64;
            let off = DVec3::new(a.cos(), a.sin(), 0.0) * (0.5 * reach);
            let size = (0.55 * reach).max(0.25);
            let half = DVec3::new(size, 0.8 * size.min(0.5 * length), size);
            push(z, off, half, Mass::Clump);
        }
    }
}

/// Draws `foliage` at `origin` into `scene`, each in its species' mesh.
pub fn draw(
    scene: &mut Scene,
    meshes: &crate::part_meshes::PartMeshes,
    origin: DVec3,
    foliage: &[Foliage],
) {
    for f in foliage {
        let Some(mesh) = meshes.foliage(usize::from(f.species), f.mass) else {
            continue;
        };
        scene.add(
            mesh,
            Instance {
                frame: Frame64::new(origin + f.centre, f.turn),
                scale: (f.half * 2.0).as_vec3(),
                material: Material::dielectric([1.0; 3], 0.8),
                heat: None,
            },
        );
    }
}
