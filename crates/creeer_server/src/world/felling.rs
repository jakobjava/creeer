//! Felling (G1.7, ADR-0031 decisions 5 and 6): a player swings at a standing tree; each blow
//! takes out the wood its energy and its edge can (`creeer_sim::trees::chop`), deepening a notch
//! on the side the player stands; when what is left of the stem no longer carries the tree above
//! it (`Tree::at_cut`), that part becomes a rigid body on a hinge where the cut went over, and
//! falls. The hinge lets go when the tree has leant through the notch's opening.

use super::strikes::Aim;
use super::*;
use creeer_math::det;
use creeer_math::glam::{DQuat, DVec2};
use creeer_physics::world::hull::Hull;
use creeer_physics::world::joint::{Joint, Kind as JointKind};
use creeer_sim::items::{Bulk, Edge, Item};
use creeer_sim::reach;
use creeer_sim::strike::Swing;
use creeer_sim::trees::{Cut, Part, Tree, chop};

/// An edge ground on a catalogue tool (m): sharp.
const GROUND_EDGE: f64 = 1e-5;
/// Wood chips lie loose, about a third solid; their pieces are a few centimetres.
const CHIPS_SOLID: f64 = 0.35;
const CHIP: f64 = 0.03;

impl ServerWorld {
    /// Avatar `by` strikes standing tree `tree` at `height` m up its stem (its swing begun at the
    /// world's tick `started`).
    pub(super) fn strike_tree(
        &mut self,
        by: EntityId,
        tree: u64,
        height: f64,
        started: f64,
    ) -> Result<(), String> {
        let ready = self.ready_to_swing(by, started, false)?;
        let (standing, grown) = self.standing(tree).ok_or("no tree stands there")?;
        if !(height.is_finite() && (0.05..grown.height).contains(&height)) {
            return Err("not on its stem".into());
        }
        // Blows go into its cut, once begun.
        let z = self.trees.cuts.get(&tree).map_or(height, |c| c.z);
        let point = standing.base + standing.turn * DVec3::new(0.0, 0.0, z);
        let i = self.index(by).ok_or("no such avatar")?;
        let centre = self.world.bodies[i].structure.position[0];
        let hands = reach::hands(centre, character::plumb(&self.world, centre));
        let far = (point - hands).length() - grown.radius(z);
        if far > reach::REACH {
            return Err(format!("{far:.1} m away: out of reach"));
        }
        self.begin_swing(by, ready, Aim::Tree(tree, z));
        Ok(())
    }

    /// What `by` strikes with: the catalogue's tool `tool`'s edge, or that of the head of what
    /// the hands hold.
    pub(super) fn striking_edge(&self, by: EntityId, tool: Option<u8>) -> Option<Edge> {
        match tool {
            Some(t) => {
                let (angle, length) = self.tools.get(usize::from(t))?.edge?;
                Some(Edge {
                    angle: angle.to_radians(),
                    length,
                    radius: GROUND_EDGE,
                })
            }
            None => self.items.bodies.get(&self.carried(by)?)?.head()?.edge,
        }
    }

    /// A blow of `swing` by `by` on standing tree `tree` at its cut's height `z`: the point it
    /// fell on, if the tree still stands and the striker still reaches it.
    pub(super) fn chop_tree(
        &mut self,
        by: EntityId,
        tree: u64,
        z: f64,
        (swing, tool): (&Swing, Option<u8>),
    ) -> Option<DVec3> {
        let (standing, grown) = self.standing(tree)?;
        let forest = Arc::clone(self.trees.forest.as_ref()?);
        let species = &forest.species[usize::from(standing.species)];
        let centre = self.world.bodies[self.index(by)?].structure.position[0];
        let axis = standing.base + standing.turn * DVec3::new(0.0, 0.0, z);
        let up = character::plumb(&self.world, centre);
        let hands = reach::hands(centre, up);
        if (axis - hands).length() - grown.radius(z) > reach::REACH {
            return None;
        }
        // The side it is cut from: towards the striker, in the tree's frame.
        let side = standing.turn.conjugate() * (centre - axis);
        let azimuth = det::atan2(side.y, side.x);
        // The energy the blow's effective mass carries into wood that does not give.
        let energy = 0.5 * swing.mass * swing.speed * swing.speed;
        let edge = self.striking_edge(by, tool);
        let volume = chop::chopped(energy, edge, species.across);
        // What a made tool's binding bears: the edge stopping over the depth it cuts, as when
        // limbing and digging (G2.3: felling had loaded it with nothing).
        let impulse = swing.mass * swing.speed;
        let force = creeer_sim::dig::peak_force(energy, volume, (impulse, super::making::CONTACT));
        self.binding_bears(by, force);
        let r = grown.radius(z);
        let at = axis + standing.turn * (DVec3::new(det::cos(azimuth), det::sin(azimuth), 0.0) * r);
        if volume <= 0.0 {
            self.note(by, "it only bruised the bark: nothing sharp".into());
            return Some(at);
        }
        let cut = self.trees.cuts.entry(tree).or_insert_with(|| Cut::at(z));
        cut.chop(azimuth, volume);
        let cut = cut.clone();
        self.trees.log.push(tree);
        match grown.at_cut(species, &cut, DVec3::ZERO) {
            Some((used, ..)) if used < 1.0 => {
                let deepest = (cut.notches(r).iter()).map(|n| n.depth).fold(0.0, f64::max);
                self.note(
                    by,
                    format!(
                        "a notch {:.0} cm deep, {:.0} % through",
                        deepest * 100.0,
                        50.0 * deepest / r
                    ),
                );
            }
            held => {
                let hinge = held.map(|h| (h.1, h.2));
                self.fell(tree, (&standing, &grown), &cut, hinge);
                self.note(by, "it goes over".into());
            }
        }
        Some(at)
    }

    /// Tree `tree` goes over at `cut`, on its hinge (the middle of its edge in the cut's plane,
    /// and the way it goes; none: nothing was left): the part above a body, held there by a
    /// hinge joint across the way it goes, as hinge wood guides a falling tree. The notch's wood
    /// lies as chips beside the stump (it came out of the standing tree, scenery).
    fn fell(
        &mut self,
        tree: u64,
        (standing, grown): (&creeer_sim::trees::Standing, &Tree),
        cut: &Cut,
        hinge: Option<(DVec2, DVec2)>,
    ) {
        let part = Part {
            species: standing.species,
            seed: standing.seed,
            dbh: standing.dbh,
            from: cut.z,
            to: grown.height,
            cut: Vec::new(),
        };
        let (_, centre) = grown.above(cut.z);
        let (pivot, toward) =
            hinge.unwrap_or((DVec2::ZERO, centre.truncate().normalize_or(DVec2::X)));
        let hull = grown.notched_hull(&part, pivot, toward);
        self.trees.cuts.remove(&tree);
        self.trees.felled.insert(tree, cut.z);
        self.trees.log.push(tree);
        let wood = cut.removed() * grown.density;
        let stump = standing.base + standing.turn * DVec3::new(0.0, 0.0, cut.z);
        self.lay_chips(&part, wood, stump);
        let Some(body) = part_body(grown, &part, &hull, (standing.base, standing.turn)) else {
            return;
        };
        let id = self.add_unlifted(body);
        self.trees.parts.insert(id, part);
        // The fallen trunk's own box goes; its stump's stays.
        self.lay_trunks();
        if hinge.is_none() {
            return;
        }
        let i = self.index(id).expect("just added");
        let at = stump + standing.turn * pivot.extend(0.0);
        let across = DVec3::new(-toward.y, toward.x, 0.0);
        let (axis, reference) = (standing.turn * across, standing.turn * DVec3::Z);
        let joint = Joint::new(
            &self.world,
            ((i, 0), None),
            (at, axis, reference),
            JointKind::Hinge { range: None },
        );
        self.world.join(joint);
        self.trees.hinged.insert(id);
    }

    /// `wood` kg of chips of `part`'s wood laid at `at`.
    pub(super) fn lay_chips(&mut self, part: &Part, wood: f64, at: DVec3) {
        let Some(forest) = self.trees.forest.as_ref() else {
            return;
        };
        let species = &forest.species[usize::from(part.species)];
        if wood <= 0.0 {
            return;
        }
        let chips = Item::Bulk(Bulk {
            mass: wood,
            composition: vec![(species.wood.clone(), 1.0)],
            temperature: 288.15,
            specific_heat: 2700.0,
            grain: CHIP,
            bulk_density: CHIPS_SOLID * species.density,
        });
        let up = character::plumb(&self.world, at);
        self.lay(chips, at, DQuat::from_rotation_arc(DVec3::Z, up));
    }
}

/// `part` of `tree` as a rigid body: `hull` its stem's (tree frame), its node at its centre of
/// mass, the tree's frame at `origin` turned by `turn`.
pub(super) fn part_body(
    tree: &Tree,
    part: &Part,
    hull: &[DVec3],
    (origin, turn): (DVec3, DQuat),
) -> Option<Body> {
    let (mass, centre, inertia) = tree.part_mass(part);
    let (shape, centroid) = Hull::new(hull).ok()?;
    let shape = shape.about(centre - centroid);
    let mut body = Body::rigid_with(
        Shape::Hull(Arc::new(shape)),
        mass,
        inertia,
        origin + turn * centre,
        DT,
    );
    body.structure.orientation[0] = turn;
    body.friction = 0.6;
    body.restitution = 0.1;
    body.solver.reset_history(&body.structure);
    Some(body)
}
