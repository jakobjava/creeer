//! Parts as members (G2.3, ADR-0033): what a buildable body is as a member (a wooden rod or
//! block item, a limbed log, or a structure built already), and fastening two of them: each made
//! a structure where it lies and as it moves, the second taken into the first, and a joint
//! between them where they touch, at nodes put into their members there.

use super::building::Built;
use super::*;
use creeer_building::assembly::{Motion, Part, Profile, assemble};
use creeer_building::fasteners::{Fastener, side};
use creeer_building::law::Placement;
use creeer_physics::structure::{ImplicitSolver, Scheme};
use creeer_physics::world::surface::Surface;
use creeer_sim::items::{Form, Item, Piece};

/// Room temperature (K): the woods' strengths a member is made with.
const ROOM: f64 = 293.15;
/// A joint this close to a member's node is made at the node (m).
const AT_NODE: f64 = 0.01;

/// A buildable body as a member, before it is one: where it lies, its wood, and the item it is.
pub(super) struct Geometry {
    pub(super) part: Part,
    pub(super) wood: String,
    pub(super) item: Item,
}

impl ServerWorld {
    /// Body `id` as a member: a wooden rod or block, a felled log with no branches on, or none
    /// if it is built already (`Ok(None)`).
    pub(super) fn part_geometry(&self, id: EntityId) -> Result<Option<Geometry>, String> {
        if self.building.built.contains_key(&id) {
            return Ok(None);
        }
        let i = self.index(id).ok_or("it is gone")?;
        let s = &self.world.bodies[i].structure;
        let (centre, turn) = (s.position[0], s.orientation[0]);
        let motion = Motion {
            centre,
            velocity: s.velocity[0],
            spin: s.angular_velocity[0],
        };
        let (start, end, up, profile, wood, item) = if let Some(item) = self.items.bodies.get(&id) {
            let Item::Piece(p, _) = item else {
                return Err("only a piece of wood is a part".into());
            };
            let (half, up, profile) = piece_profile(p)?;
            let (start, end) = (centre - turn * half, centre + turn * half);
            (
                start,
                end,
                turn * up,
                profile,
                p.material.to_string(),
                item.clone(),
            )
        } else if let Some(part) = self.trees.parts.get(&id) {
            let forest = self.trees.forest.as_ref().ok_or("no trees")?;
            let tree = forest.grow_part(part).ok_or("no such tree")?;
            if !tree.kept(part).is_empty() || self.trees.hinged.contains(&id) {
                return Err("limb it first: its branches are still on".into());
            }
            let (_, mass_centre, _) = tree.part_mass(part);
            let top = part.to.min(tree.height);
            let world = |z: f64| centre + turn * (DVec3::new(0.0, 0.0, z) - mass_centre);
            let (d0, d1) = (2.0 * tree.radius(part.from), 2.0 * tree.radius(top));
            let wood = forest.species[usize::from(part.species)].wood.clone();
            let log = Item::Piece(
                Piece::solid(
                    &wood,
                    self.wood_density(&wood)?,
                    log_form(0.25 * (d0 + d1), top - part.from),
                ),
                Vec::new(),
            );
            let profile = Profile::Round { start: d0, end: d1 };
            (
                world(part.from),
                world(top),
                turn * DVec3::X,
                profile,
                wood,
                log,
            )
        } else {
            return Err("it is no part to build with".into());
        };
        let wood_material = self.wood(&wood)?;
        side(wood_material, 0.1)?;
        let material = wood_material.structural(ROOM).into();
        Ok(Some(Geometry {
            part: Part {
                start,
                end,
                up,
                profile,
                material,
                motion,
            },
            wood,
            item,
        }))
    }

    pub(super) fn wood(&self, id: &str) -> Result<&creeer_materials::Material, String> {
        (self.building.materials.as_ref())
            .and_then(|m| m.get(id))
            .ok_or_else(|| format!("no material {id}"))
    }

    fn wood_density(&self, id: &str) -> Result<f64, String> {
        self.wood(id).map(|w| w.density)
    }

    /// Body `id` made a structure where it lies (if it is not one yet), kept under its id.
    pub(super) fn make_built(&mut self, id: EntityId) -> Result<(), String> {
        let Some(g) = self.part_geometry(id)? else {
            return Ok(());
        };
        let built = assemble(std::slice::from_ref(&g.part), &[])?;
        let i = self.index(id).ok_or("it is gone")?;
        let mut body = Body::structure(built.structure, DT);
        body.shapes = built.shapes;
        body.surface = Surface::of(&g.part.material);
        body.friction = self.world.bodies[i].friction;
        if let Some(planet) = &self.world.planet {
            body.structure.planet = Some(planet.clone());
            body.structure.gravity = DVec3::ZERO;
        }
        self.world.bodies[i] = body;
        self.world.bodies[i].solver =
            ImplicitSolver::new(&self.world.bodies[i].structure, Scheme::Newmark, DT, 1);
        self.items.bodies.remove(&id);
        self.trees.parts.remove(&id);
        self.trees.bucks.remove(&id);
        self.trees.limbing.remove(&id);
        let woods = vec![Some(g.wood); self.world.bodies[i].structure.element_count()];
        self.building.built.insert(
            id,
            Built {
                items: vec![g.item],
                woods,
            },
        );
        Ok(())
    }

    /// Fastens part `x` to part `y` where they touch at `point`, with one of `fastener`: one
    /// structure under `x`'s id.
    pub(super) fn fasten(
        &mut self,
        x: EntityId,
        y: EntityId,
        point: DVec3,
        fastener: &Fastener,
    ) -> Result<(), String> {
        self.make_built(x)?;
        self.make_built(y)?;
        // Whether the joint would hold, the parts still apart: refused after they were made one,
        // they stayed one with no joint, and the hands lifted both (G2.10). Refused, the hands
        // set them down where they rest (made structures, their nodes are not the one gripped).
        if x != y
            && let Err(why) = self.would_hold(x, y, point, fastener)
        {
            self.hands_off(&[x, y]);
            return Err(why);
        }
        let xi = self.index(x).ok_or("it is gone")?;
        let first = self.world.bodies[xi].structure.element_count();
        if x != y {
            let theirs = self.remove_entity(y).ok_or("what it touches is gone")?;
            let xi = self.index(x).ok_or("it is gone")?;
            let body = &mut self.world.bodies[xi];
            let offset = body.structure.append(&theirs.structure);
            body.shapes
                .extend(theirs.shapes.iter().map(|(n, s)| (n + offset, s.clone())));
            let record = self.building.built.remove(&y).unwrap_or_default();
            let mine = self.building.built.entry(x).or_default();
            mine.items.extend(record.items);
            mine.woods.extend(record.woods);
        }
        let xi = self.index(x).ok_or("it is gone")?;
        let end = self.world.bodies[xi].structure.element_count();
        if x == y {
            return Err("that is one part".into());
        }
        // The nodes put in where they cross change the structure: its solver is made anew
        // whether the joint is then made or refused (refused after them, a nail too short for
        // the parts, the next step met a node its solver did not have and the server fell over:
        // G2.10).
        let joined = (|| {
            let (a, ea, ta) = self.node_at(x, point, 0..first)?;
            let (b, eb, tb) = self.node_at(x, point, first..end)?;
            let law = self.joint_law(x, [(a, ea, ta), (b, eb, tb)], fastener)?;
            self.world.bodies[xi].structure.join(a, b, &law);
            Ok::<(), String>(())
        })();
        let body = &mut self.world.bodies[xi];
        body.solver = ImplicitSolver::new(&body.structure, Scheme::Newmark, DT, 1);
        body.wake();
        if let Some(r) = self.building.built.get_mut(&x) {
            r.woods
                .resize(self.world.bodies[xi].structure.element_count(), None);
        }
        joined?;
        if let Some(r) = self.building.built.get_mut(&x) {
            r.woods
                .resize(self.world.bodies[xi].structure.element_count(), None);
        }
        Ok(())
    }

    /// The law of a joint of `fastener` in built structure `x` between two of its members' nodes,
    /// each (node, member, thickness).
    pub(super) fn joint_law(
        &self,
        x: EntityId,
        [(a, ea, ta), (b, eb, tb)]: [(usize, usize, f64); 2],
        fastener: &Fastener,
    ) -> Result<creeer_physics::structure::JointLaw, String> {
        let s = &self.world.bodies[self.index(x).ok_or("it is gone")?].structure;
        let between = (s.position[b] - s.position[a]).length();
        let record = self.building.built.get(&x).ok_or("no record")?;
        let wood_of = |e: usize| {
            (record.woods.get(e).cloned().flatten()).ok_or_else(|| "a joint is no part".to_owned())
        };
        let (wa, wb) = (wood_of(ea)?, wood_of(eb)?);
        let materials = self.building.materials.as_ref().ok_or("no materials")?;
        let sa = side(self.wood(&wa)?, ta)?;
        let sb = side(self.wood(&wb)?, tb)?;
        // The two bear on each other over about the narrower's thickness squared (an estimate).
        let at = Placement {
            count: 1,
            spread: 0.0,
            contact: ta.min(tb) * ta.min(tb),
        };
        fastener.law(materials, (&sa, &sb), &at, between)
    }

    /// The node of built structure `id`'s members `elements` (a range of its elements, joints
    /// skipped) nearest `point`, put into the member there if no node is near; that member and
    /// its thickness.
    pub(super) fn node_at(
        &mut self,
        id: EntityId,
        point: DVec3,
        elements: std::ops::Range<usize>,
    ) -> Result<(usize, usize, f64), String> {
        let i = self.index(id).ok_or("it is gone")?;
        let s = &self.world.bodies[i].structure;
        let (e, (a, b, thick), t, length) = (elements.filter_map(|e| s.member(e).map(|m| (e, m))))
            .map(|(e, m)| {
                let (pa, pb) = (s.position[m.0], s.position[m.1]);
                let d = pb - pa;
                let t = ((point - pa).dot(d) / d.length_squared()).clamp(0.0, 1.0);
                (e, m, t, d.length(), (pa + d * t - point).length())
            })
            .min_by(|x, y| x.4.total_cmp(&y.4))
            .map(|(e, m, t, l, _)| (e, m, t, l))
            .ok_or("no member there")?;
        let node = if t * length < AT_NODE {
            a
        } else if (1.0 - t) * length < AT_NODE {
            b
        } else {
            let n = self.world.bodies[i].structure.insert_node(e, t);
            // The member's second piece is the structure's last element: of the member's wood.
            if let Some(r) = self.building.built.get_mut(&id) {
                let wood = r.woods.get(e).cloned().flatten();
                r.woods.push(wood);
            }
            n
        };
        Ok((node, e, thick))
    }
}

/// A piece as a member: half its length along its axis (in its frame), its up, its profile.
fn piece_profile(p: &Piece) -> Result<(DVec3, DVec3, Profile), String> {
    match p.form {
        Form::Rod { radius, length } => Ok((
            DVec3::X * (0.5 * length),
            DVec3::Y,
            Profile::Round {
                start: 2.0 * radius,
                end: 2.0 * radius,
            },
        )),
        Form::Block { size } => {
            // Along its longest side; its width the next.
            let mut axes = [(size.x, DVec3::X), (size.y, DVec3::Y), (size.z, DVec3::Z)];
            axes.sort_by(|a, b| b.0.total_cmp(&a.0));
            Ok((
                axes[0].1 * (0.5 * axes[0].0),
                axes[1].1,
                Profile::Board {
                    width: axes[1].0,
                    depth: axes[2].0,
                },
            ))
        }
        _ => Err("only a rod or a block is a part".into()),
    }
}

/// A log's form as an item: a rod of its mean radius.
fn log_form(radius: f64, length: f64) -> Form {
    Form::Rod { radius, length }
}

impl ServerWorld {
    /// Whether a joint of `fastener` between built structures `x` and `y` where they touch at
    /// `point` would hold (its law could be made), found without changing either: each one's
    /// member nearest `point`, the point on its axis there and its thickness.
    fn would_hold(
        &self,
        x: EntityId,
        y: EntityId,
        point: DVec3,
        fastener: &Fastener,
    ) -> Result<(), String> {
        let near = |id: EntityId| -> Result<(String, f64, DVec3), String> {
            let s = &self.world.bodies[self.index(id).ok_or("it is gone")?].structure;
            let (e, thick, at) = (0..s.element_count())
                .filter_map(|e| s.member(e).map(|m| (e, m)))
                .map(|(e, (a, b, thick))| {
                    let (pa, pb) = (s.position[a], s.position[b]);
                    let d = pb - pa;
                    let t = ((point - pa).dot(d) / d.length_squared()).clamp(0.0, 1.0);
                    (e, thick, pa + d * t)
                })
                .min_by(|p, q| (p.2 - point).length().total_cmp(&(q.2 - point).length()))
                .ok_or("no member there")?;
            let wood = (self.building.built.get(&id))
                .and_then(|r| r.woods.get(e).cloned().flatten())
                .ok_or("a joint is no part")?;
            Ok((wood, thick, at))
        };
        let ((wa, ta, pa), (wb, tb, pb)) = (near(x)?, near(y)?);
        let materials = self.building.materials.as_ref().ok_or("no materials")?;
        let at = Placement {
            count: 1,
            spread: 0.0,
            contact: ta.min(tb) * ta.min(tb),
        };
        let (sa, sb) = (side(self.wood(&wa)?, ta)?, side(self.wood(&wb)?, tb)?);
        fastener.law(materials, (&sa, &sb), &at, (pb - pa).length())?;
        Ok(())
    }
}
