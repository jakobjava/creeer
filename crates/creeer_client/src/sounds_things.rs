//! The things the client is told of, as `creeer_audio` sounds them (ADR-0037): a body's material
//! known by its Young's modulus and density (`Matter`), a felled part's by its species' wood, a
//! structure's members by the material structures sound as (the client is not told theirs), the
//! ground by its rock; a tool's blow and a fastener's.

use crate::sounds::Heard;
use creeer_audio::events::{Body, Other, driven, strike};
use creeer_audio::library::Ground;
use creeer_audio::modes::Shape as Ringing;
use creeer_audio::{Library, VoiceSpec};
use creeer_math::glam::{DQuat, DVec3};
use creeer_net::entity::{EntityId, Frame, Kind, Shape};
use creeer_net::quant::POSITION_SCALE;
use creeer_server::Connection;
use creeer_terrain::Terrain;
use std::f64::consts::PI;

/// What sounds need of the packs besides the sound data: each tree species' wood, each
/// fastener's material and size, the tools.
#[derive(Clone, Debug, Default)]
pub struct Extra {
    woods: Vec<String>,
    fasteners: Vec<(String, String, f64, f64)>,
    tools: Vec<creeer_sim::tools::Tool>,
}

impl Extra {
    pub fn load(data: &creeer_data::Data) -> Self {
        use creeer_building::fasteners::{Fastener, Kind as F};
        let woods = (creeer_sim::trees::species::load(data)
            .unwrap_or_default()
            .into_iter())
        .map(|s| s.wood)
        .collect();
        let fasteners = (Fastener::all(data).unwrap_or_default().into_iter())
            .filter_map(|f| match f.kind {
                F::Nail {
                    diameter, length, ..
                }
                | F::Peg { diameter, length } => Some((f.name, f.material, diameter, length)),
                F::Lashing { .. } => None,
            })
            .collect();
        let tools = creeer_sim::tools::Tool::all(data).unwrap_or_default();
        Self {
            woods,
            fasteners,
            tools,
        }
    }
}

/// Where entity `id` is as the newest snapshot has it (its node 0 for a structure).
fn pose(c: &Connection, id: EntityId) -> Option<(DVec3, DQuat)> {
    let s = c.receiver().newest()?.states.get(&id)?;
    Some((s.position.value(POSITION_SCALE), s.rotation.value()))
}

/// The ground at `p`: its rock's sound.
pub fn ground<'l>(lib: &'l Library, terrain: &Terrain, p: DVec3) -> &'l Ground {
    let rocks = &terrain.planet().geology.rocks.rocks;
    let name = (terrain.rock_at(p))
        .and_then(|r| rocks.get(usize::from(r)))
        .map_or("default", |r| r.name.as_str());
    lib.ground(name)
}

/// Member `member` of structure `id` (of `frame`, as it is or was), as it rings.
pub fn member<'l>(
    lib: &'l Library,
    c: &Connection,
    (id, frame): (EntityId, &Frame),
    member: u32,
) -> Option<Body<'l>> {
    let (p, q) = pose(c, id)?;
    let [a, b] = *frame.members.get(member as usize)?;
    let (a, b) = (frame.nodes.get(a as usize)?, frame.nodes.get(b as usize)?);
    let radius = frame
        .radii
        .get(member as usize)
        .copied()
        .unwrap_or(frame.radius);
    (radius > 0.0).then(|| Body::member(lib.structures(), (p + q * *a, p + q * *b), radius))
}

/// Of `old`'s members, the largest that `now` (the same structure, come apart) no longer has,
/// matched by where their nodes are (within a centimetre).
pub fn gone(old: &Frame, now: &Frame) -> Option<u32> {
    let at =
        |f: &Frame, [a, b]: [u32; 2]| Some((*f.nodes.get(a as usize)?, *f.nodes.get(b as usize)?));
    let kept = |x: DVec3, y: DVec3| {
        (now.members.iter())
            .filter_map(|m| at(now, *m))
            .any(|(u, v)| {
                (u.distance(x) < 0.01 && v.distance(y) < 0.01)
                    || (u.distance(y) < 0.01 && v.distance(x) < 0.01)
            })
    };
    let radius = |k: usize| old.radii.get(k).copied().unwrap_or(old.radius);
    (0..old.members.len())
        .filter(|&k| radius(k) > 0.0)
        .filter(|&k| at(old, old.members[k]).is_some_and(|(x, y)| !kept(x, y)))
        .max_by(|&a, &b| radius(a).total_cmp(&radius(b)))
        .and_then(|k| u32::try_from(k).ok())
}

/// A log lying along the longest side of a box (a felled part's hull about it).
fn log<'l>(
    m: &'l creeer_audio::library::Matter,
    half: DVec3,
    (centre, turn): (DVec3, DQuat),
) -> Body<'l> {
    let (shape, order) = Ringing::of_box(half.to_array());
    let axis = turn * [DVec3::X, DVec3::Y, DVec3::Z][order[0]];
    let length = match shape {
        Ringing::Bar { length, .. } => length,
        Ringing::Ball { radius } => 2.0 * radius,
    };
    let radius = 0.5 * (half[order[1]] + half[order[2]]);
    Body::member(
        m,
        (centre - axis * 0.5 * length, centre + axis * 0.5 * length),
        radius,
    )
}

/// The box about a hull's corners: its centre and half extents.
fn about(corners: &[DVec3]) -> (DVec3, DVec3) {
    let (lo, hi) = corners
        .iter()
        .fold((DVec3::MAX, DVec3::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    ((lo + hi) * 0.5, (hi - lo) * 0.5)
}

/// Entity `id` as it rings, posed at `(p, q)`; a structure as its member nearest `near`.
pub fn sounding<'l>(
    lib: &'l Library,
    extra: &Extra,
    c: &Connection,
    id: EntityId,
    near: DVec3,
) -> Option<Body<'l>> {
    let (p, q) = pose(c, id)?;
    match c.kinds.get(&id)? {
        Kind::Body(shape, matter) => {
            let (half, centre, volume) = match shape {
                Shape::Box(half) => (*half, p, 8.0 * half.x * half.y * half.z),
                Shape::Sphere(r) => (DVec3::splat(*r), p, 4.0 / 3.0 * PI * r * r * r),
                // A pebble or a stick fills about half the box about it.
                Shape::Hull(corners) => {
                    let (middle, half) = about(corners);
                    (half, p + q * middle, 4.0 * half.x * half.y * half.z)
                }
            };
            let m = lib.nearest(matter.surface[0], matter.mass / volume.max(1e-9));
            Some(Body::boxed(m, half, (centre, q), Some(matter.mass)))
        }
        Kind::Tree(part) => {
            let wood = extra
                .woods
                .get(usize::from(part.species))
                .and_then(|w| lib.matter(w));
            let (middle, half) = about(&part.hull);
            Some(log(
                wood.unwrap_or(lib.structures()),
                half,
                (p + q * middle, q),
            ))
        }
        Kind::Structure(frame) => {
            let nearest = (0..frame.members.len()).min_by(|&a, &b| {
                let d = |k: usize| {
                    let [i, j] = frame.members[k];
                    let (x, y) = (
                        p + q * frame.nodes[i as usize],
                        p + q * frame.nodes[j as usize],
                    );
                    distance_to_segment(near, x, y)
                };
                d(a).total_cmp(&d(b))
            })?;
            member(lib, c, (id, frame), u32::try_from(nearest).ok()?)
        }
        Kind::Character { .. } | Kind::Segment { .. } => None,
    }
}

fn distance_to_segment(p: DVec3, a: DVec3, b: DVec3) -> f64 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The part entity `id` is (that a fastener goes into) near `point`.
pub fn part<'l>(
    lib: &'l Library,
    extra: &Extra,
    c: &Connection,
    id: EntityId,
    point: DVec3,
) -> Option<Body<'l>> {
    sounding(lib, extra, c, id, point)
}

/// How far `p` is from `body`'s surface (m).
fn gap(body: &Body<'_>, p: DVec3) -> f64 {
    match body.shape {
        Ringing::Bar {
            length,
            width,
            depth,
            ..
        } => {
            let axis = body.turn * DVec3::X * (0.5 * length);
            distance_to_segment(p, body.centre - axis, body.centre + axis) - 0.5 * width.min(depth)
        }
        Ringing::Ball { radius } => p.distance(body.centre) - radius,
    }
}

/// The body (or structure's member) nearest `near`, within 0.3 m of it, but `except`.
pub fn nearest<'l>(
    lib: &'l Library,
    extra: &Extra,
    c: &Connection,
    near: DVec3,
    except: Option<EntityId>,
) -> Option<Body<'l>> {
    let mut best: Option<(f64, Body<'l>)> = None;
    for (id, kind) in &c.kinds {
        if Some(*id) == except
            || !matches!(kind, Kind::Structure(_) | Kind::Body(..) | Kind::Tree(_))
        {
            continue;
        }
        // (A structure's node 0 may lie far from its members; a body's centre is near it.)
        let far = if matches!(kind, Kind::Structure(_)) {
            30.0
        } else {
            6.0
        };
        if pose(c, *id).is_none_or(|(p, _)| p.distance(near) > far) {
            continue;
        }
        if let Some(body) = sounding(lib, extra, c, *id, near) {
            let d = gap(&body, near);
            if d < 0.3 && best.as_ref().is_none_or(|b| d < b.0) {
                best = Some((d, body));
            }
        }
    }
    best.map(|b| b.1)
}

/// What a foot at `at` stands on if not the ground: a structure's member or a body under it.
pub fn underfoot<'l>(
    lib: &'l Library,
    extra: &Extra,
    h: &Heard<'_>,
    at: DVec3,
    up: DVec3,
) -> Option<Body<'l>> {
    if h.terrain.density(at) < 0.08 {
        return None;
    }
    nearest(lib, extra, h.connection, at - up * 0.1, None)
}

/// The player's tool's blow on what the crosshair is on.
pub fn tool_blow(
    lib: &Library,
    extra: &Extra,
    h: &Heard<'_>,
    (point, body): (DVec3, Option<EntityId>),
    seed: u64,
) -> Option<VoiceSpec> {
    let tool = extra.tools.get(usize::from(h.wielding?))?;
    let swing = creeer_sim::strike::swing(tool);
    // The head as a steel block of its mass, struck along the line from the eyes.
    let side = creeer_math::det::cbrt(tool.head / lib.tools().acoustic.density);
    let half = DVec3::new(1.5 * side, 0.5 * side, 0.4 * side);
    let along = (point - h.ears.0).normalize_or(DVec3::NEG_Y);
    let head = Body::boxed(
        lib.tools(),
        half,
        (
            point - along * half.x,
            DQuat::from_rotation_arc(DVec3::X, along),
        ),
        Some(swing.mass),
    );
    let target = body.and_then(|id| sounding(lib, extra, h.connection, id, point));
    let other = match target {
        Some(b) => Other::Body(b),
        None => Other::Ground(ground(lib, h.terrain, point - along * 0.05)),
    };
    Some(strike(&head, &other, point, along * swing.speed, seed))
}

/// Blow `k` driving `fastener` into `part` (or, unknown, a log of the structures' wood).
pub fn blow(
    lib: &Library,
    extra: &Extra,
    (fastener, k): (&str, u32),
    part: Option<Body<'_>>,
    (point, axis): (DVec3, DVec3),
    seed: u64,
) -> Option<VoiceSpec> {
    let driver = lib.driver(fastener)?;
    let (_, material, diameter, length) = extra.fasteners.iter().find(|f| f.0 == fastener)?;
    let rod = lib.matter(material)?;
    let fallback = Body::member(lib.structures(), (point - DVec3::X, point + DVec3::X), 0.1);
    let part = part.unwrap_or(fallback);
    Some(driven(
        lib,
        driver,
        (rod, *diameter, *length),
        &part,
        (point, axis),
        k,
        seed,
    ))
}
