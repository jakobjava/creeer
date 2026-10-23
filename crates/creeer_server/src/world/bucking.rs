//! Limbing and bucking (G1.7, ADR-0031 decision 7): blows on a felled tree's part. Near a node
//! whose branches are still on, a blow's wood goes into them, thinnest first, each off once a
//! notch through it is paid for; it lies beside the trunk as a piece (a haft, firewood). Else it
//! goes into a cut across the stem, from the striker's side; once what is left of the section is
//! under a fiftieth of it, the last fibres tear and the part is two. Either way the part is a new
//! body (its mass changed), moving as the old one did.

use super::felling::part_body;
use super::*;
use creeer_math::det;
use creeer_math::glam::{DMat3, DQuat};
use creeer_sim::items::{Form, Item, Piece};
use creeer_sim::trees::section::Section;
use creeer_sim::trees::{Cut, Part, Tree, chop};

/// What is left of a section when its last fibres tear, over the whole.
const TEARS_AT: f64 = 0.02;
/// Pieces shorter than this are chips (m).
const SHORTEST: f64 = 0.02;

impl ServerWorld {
    /// A blow of `swing` (with tool `tool`) by `by` on felled part `id` at world point `at`,
    /// travelling along `way`, delivering `energy` (J).
    pub(super) fn chop_part(
        &mut self,
        by: EntityId,
        id: EntityId,
        (at, way): (DVec3, DVec3),
        (energy, tool): (f64, Option<u8>),
    ) {
        let Some(part) = self.trees.parts.get(&id).cloned() else {
            return;
        };
        let Some(forest) = self.trees.forest.clone() else {
            return;
        };
        let Some(tree) = forest.grow_part(&part) else {
            return;
        };
        let species = &forest.species[usize::from(part.species)];
        let Some(i) = self.index(id) else { return };
        let s = &self.world.bodies[i].structure;
        let (position, turn) = (s.position[0], s.orientation[0]);
        let (_, centre, _) = tree.part_mass(&part);
        let local = centre + turn.conjugate() * (at - position);
        let z = local.z.clamp(part.from, part.to);
        let volume = chop::chopped(energy, self.striking_edge(by, tool), species.across);
        if volume <= 0.0 {
            self.note(by, "it only bruised the bark: nothing sharp".into());
            return;
        }
        // Limbing: the nearest node within a spacing whose branches are still on.
        let spacing = species.branching.spacing;
        let node = (0..tree.nodes.len())
            .filter(|&k| {
                let n = &tree.nodes[k];
                n.z >= part.from
                    && n.z < part.to
                    && part.off(k) < n.branches.len()
                    && (n.z - z).abs() <= spacing
            })
            .min_by(|a, b| {
                let d = |k: usize| (tree.nodes[k].z - z).abs();
                d(*a).total_cmp(&d(*b))
            });
        if let Some(k) = node {
            self.limb(by, id, (&part, &tree), k, volume);
            return;
        }
        let from = turn.conjugate() * -way;
        let azimuth = det::atan2(from.y, from.x);
        self.buck(by, id, (&part, &tree), (z, azimuth), volume);
    }

    /// `volume` m³ of wood taken out of node `k`'s branches on part `id`.
    fn limb(
        &mut self,
        by: EntityId,
        id: EntityId,
        (part, tree): (&Part, &Tree),
        k: usize,
        volume: f64,
    ) {
        let progress = self.trees.limbing.entry(id).or_insert((k, 0.0));
        if progress.0 != k {
            *progress = (k, 0.0);
        }
        progress.1 += volume;
        let node = &tree.nodes[k];
        let order = Tree::cut_order(node);
        let mut off = part.off(k);
        let mut came = Vec::new();
        while let Some(&b) = order.get(off) {
            let branch = &node.branches[b];
            let need = chop::notch_volume(branch.radius, 2.0 * branch.radius);
            if progress.1 < need {
                break;
            }
            progress.1 -= need;
            off += 1;
            came.push(*branch);
        }
        if came.is_empty() {
            self.note(by, "a cut into a branch".into());
            return;
        }
        let mut limbed = part.clone();
        limbed.cut.resize(tree.nodes.len(), 0);
        limbed.cut[k] = u8::try_from(off).unwrap_or(u8::MAX);
        let Some(i) = self.index(id) else { return };
        let s = &self.world.bodies[i].structure;
        let (position, turn) = (s.position[0], s.orientation[0]);
        let (_, centre, _) = tree.part_mass(part);
        let density = tree.density;
        for b in &came {
            // The branch as a rod of its wood, its mass its own (needles and all).
            let radius = (b.mass() / (density * std::f64::consts::PI * b.length)).sqrt();
            let piece = Piece::solid(
                &self.trees.forest.as_ref().map_or_else(String::new, |f| {
                    f.species[usize::from(part.species)].wood.clone()
                }),
                density,
                Form::Rod {
                    radius,
                    length: b.length,
                },
            );
            let middle = DVec3::new(0.0, 0.0, node.z) + b.direction() * (0.5 * b.length);
            let at = position + turn * (middle - centre);
            let up = character::plumb(&self.world, at);
            let along = turn * b.direction();
            let flat = (along - up * along.dot(up)).normalize_or(up.any_orthonormal_vector());
            let frame = DQuat::from_mat3(&DMat3::from_cols(flat, up.cross(flat), up));
            self.lay(Item::Piece(piece, Vec::new()), at, frame);
        }
        self.trees.limbing.remove(&id);
        self.replace_part(id, part, tree, vec![limbed]);
        let n = came.len();
        self.note(
            by,
            if n == 1 {
                "a branch came off".into()
            } else {
                format!("{n} branches came off")
            },
        );
    }

    /// `volume` m³ more out of part `id`'s cut at `z` (begun there, if none is), from side
    /// `azimuth`; through, the part is two.
    fn buck(
        &mut self,
        by: EntityId,
        id: EntityId,
        (part, tree): (&Part, &Tree),
        (z, azimuth): (f64, f64),
        volume: f64,
    ) {
        let cut = self.trees.bucks.entry(id).or_insert_with(|| Cut::at(z));
        cut.chop(azimuth, volume);
        let cut = cut.clone();
        let r = tree.radius(cut.z);
        let whole = std::f64::consts::PI * r * r;
        let left = Section::cut(r, &cut.notches(r)).map_or(0.0, |s| s.area);
        if left > TEARS_AT * whole {
            self.note(by, format!("{:.0} % through", 100.0 * (1.0 - left / whole)));
            return;
        }
        let Some(i) = self.index(id) else { return };
        let s = &self.world.bodies[i].structure;
        let (position, turn) = (s.position[0], s.orientation[0]);
        let (_, centre, _) = tree.part_mass(part);
        let (start, width) = kerf(tree, part, cut.z, cut.removed() * tree.density);
        let mut chips = tree.stem_exact(start, start + width).0;
        let mut pieces = Vec::new();
        for (from, to) in [(part.from, start), (start + width, part.to)] {
            let piece = Part {
                from,
                to,
                ..part.clone()
            };
            if to - from >= SHORTEST {
                pieces.push(piece);
            } else {
                chips += tree.part_mass(&piece).0;
            }
        }
        let at = position + turn * (DVec3::new(0.0, 0.0, start + 0.5 * width) - centre);
        self.lay_chips(part, chips, at);
        self.replace_part(id, part, tree, pieces);
        self.note(by, "cut through".into());
    }

    /// Part `id` (as `part` of `tree`) replaced by `pieces`, each a body moving as it moved.
    fn replace_part(&mut self, id: EntityId, part: &Part, tree: &Tree, pieces: Vec<Part>) {
        let (_, centre, _) = tree.part_mass(part);
        let Some(old) = self.remove_entity(id) else {
            return;
        };
        let s = &old.structure;
        let (position, turn) = (s.position[0], s.orientation[0]);
        let (velocity, spin) = (s.velocity[0], s.angular_velocity[0]);
        let origin = position - turn * centre;
        for piece in pieces {
            let hull = tree.part_hull(&piece);
            let Some(mut body) = part_body(tree, &piece, &hull, (origin, turn)) else {
                continue;
            };
            let at = body.structure.position[0];
            body.structure.velocity[0] = velocity + spin.cross(at - position);
            body.structure.angular_velocity[0] = spin;
            body.solver.reset_history(&body.structure);
            let new = self.add_unlifted(body);
            self.trees.parts.insert(new, piece);
        }
    }
}

/// The kerf a cut at `z` into `part` leaves, `mass` kg of its stem: where it begins and how
/// wide it is, a slice of exactly that mass (so the pieces and the chips are the part, to
/// rounding), moved above any node whose branches are still on (they stay with the piece above).
fn kerf(tree: &Tree, part: &Part, z: f64, mass: f64) -> (f64, f64) {
    let width = |s: f64| {
        let (mut lo, mut hi) = (0.0, (part.to - s).max(0.0));
        if tree.stem_exact(s, part.to).0 <= mass {
            return hi;
        }
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            if tree.stem_exact(s, s + mid).0 < mass {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        hi
    };
    let mut start = z.clamp(part.from, part.to);
    for _ in 0..4 {
        let w = width(start);
        let held = (tree.nodes.iter().enumerate())
            .find(|(k, n)| n.z >= start && n.z < start + w && part.off(*k) < n.branches.len());
        match held {
            Some((_, n)) => start = n.z + 1e-6,
            None => return (start, w),
        }
    }
    (start, width(start))
}
