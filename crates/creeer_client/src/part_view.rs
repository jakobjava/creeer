//! Things drawn as what they are (G2.11, ADR-0037): a rod round in its wood, a board sawn, a
//! stone a speckled lump in its rock's colour, a made tool its haft and head, a dead branch with
//! its bark. The drawn shape is the simulated one (`art-style.md` §2): a rod's hull is its
//! sixteen-sided prism (`creeer_server`'s `item_bodies.rs`), drawn as the round it stands for;
//! a stone's an ellipsoid. What a thing is made of shows by its token (`part_meshes.rs`).

use crate::part_meshes::PartMeshes;
use creeer_math::Frame64;
use creeer_math::glam::{DQuat, DVec3, Vec3};
use creeer_render::{Instance, Material, MeshId, Scene};

/// A thing's shape, in its own frame: a rod along its x axis (its top `taper` times as thick as
/// its bottom), a block of these half extents, a lump (an ellipsoid) of these half axes, or a
/// piece told only by its box (a made thing's, a thing laid: by its material, see [`piece`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Form {
    Rod { radius: f64, length: f64, taper: f64 },
    Block(DVec3),
    Lump(DVec3),
    Piece(DVec3),
}

/// What a thing is made of, as drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Made {
    /// A material or a rock by its colour (an item's: `Matter::colour`).
    Colour([u8; 3]),
    /// A log of species `k` (bark, cut ends).
    Log(u16),
    /// Dead wood lying about.
    Deadwood,
}

/// A thing to draw: where, turned how, its form, what it is made of, a key that makes it its
/// own (its tone and grain), and its age (years weathered, wear).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thing {
    pub centre: DVec3,
    pub turn: DQuat,
    pub form: Form,
    pub made: Made,
    pub key: u64,
    pub age: (f32, f32),
}

/// How a rod's hull is told (a prism along x: both its ends' rings of one radius), if it is one:
/// its radius and length.
pub fn rod_of(corners: &[DVec3]) -> Option<(f64, f64)> {
    let half = corners.iter().map(|c| c.x.abs()).fold(0.0, f64::max);
    let radius = (corners.iter())
        .map(|c| c.y.hypot(c.z))
        .fold(0.0, f64::max);
    let all = corners.len() >= 12
        && corners.iter().all(|c| {
            (c.x.abs() - half).abs() <= 1e-3 * half && (c.y.hypot(c.z) - radius).abs() <= 1e-3 * radius
        });
    (all && half > 0.0 && radius > 0.0).then_some((radius, 2.0 * half))
}

/// An item body of hull `corners` (about its centre `p`, turned by `q`) and `colour`, with the
/// pieces it is made of if any (each at, turned, half extents, colour, about its centre), as the
/// things it is drawn as.
pub fn item(
    out: &mut Vec<Thing>,
    (p, q): (DVec3, DQuat),
    corners: &[DVec3],
    colour: [u8; 3],
    pieces: &[creeer_net::message::Part],
    key: u32,
) {
    let key = u64::from(key);
    if !pieces.is_empty() {
        for (k, part) in pieces.iter().enumerate() {
            out.push(Thing {
                centre: p + q * part.at,
                turn: q * part.turn,
                form: Form::Piece(part.half),
                made: Made::Colour(part.colour),
                key: key.wrapping_mul(31).wrapping_add(k as u64),
                age: (0.0, 0.0),
            });
        }
        return;
    }
    let (lo, hi) = (corners.iter()).fold((DVec3::MAX, DVec3::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let form = match rod_of(corners) {
        Some((radius, length)) => Form::Rod {
            radius,
            length,
            taper: 1.0,
        },
        None => Form::Lump((hi - lo) * 0.5),
    };
    out.push(Thing {
        centre: p + q * ((lo + hi) * 0.5),
        turn: q,
        form,
        made: Made::Colour(colour),
        key,
        age: (0.0, 0.0),
    });
}

/// An item body that is a block of `half` extents of the material coloured `colour` (a sill, a
/// crate, a strip), about `p` turned by `q`, `age` weathered.
pub fn block(key: u32, (p, q): (DVec3, DQuat), half: DVec3, colour: [u8; 3], age: (f32, f32)) -> Thing {
    Thing {
        centre: p,
        turn: q,
        form: Form::Block(half),
        made: Made::Colour(colour),
        key: u64::from(key),
        age,
    }
}

/// A piece of a made thing (or a thing laid) by its box and whether it is `wooden`: wood with a
/// round section is a rod (a haft, a stick: items are made as rods, never square), other wood a
/// block (a strip), anything else a lump (a stone).
pub fn piece(half: DVec3, wooden: bool) -> Form {
    if wooden && (half.y - half.z).abs() <= 1e-3 * half.y.max(half.z) && half.x > half.y {
        Form::Rod {
            radius: half.y,
            length: 2.0 * half.x,
            taper: 1.0,
        }
    } else if wooden {
        Form::Block(half)
    } else {
        Form::Lump(half)
    }
}

/// A key's tone and seed (0–1 each): no two parts alike (`art-style.md` §6, layer 1).
fn tone_seed(key: u64) -> (f32, f32) {
    let h = key
        .wrapping_add(0x9e37_79b9_7f4a_7c15)
        .wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let h = (h ^ (h >> 31)).wrapping_mul(0x94d0_49bb_1331_11eb);
    #[allow(clippy::cast_precision_loss)] // 24 bits each
    let unit = |s: u32| ((h >> s) & 0xff_ffff) as f32 / 16_777_215.0;
    (unit(0), unit(24))
}

/// The mesh, frame and scale `thing` is drawn with, if `meshes` has one for it.
pub fn placed(thing: &Thing, meshes: &PartMeshes) -> Option<(MeshId, Frame64, Vec3, f32)> {
    // The unit cylinder stands along Y; a rod lies along its frame's x.
    let lying = DQuat::from_rotation_z(-std::f64::consts::FRAC_PI_2);
    let form = match (thing.form, thing.made) {
        (Form::Piece(half), Made::Colour(c)) => piece(half, meshes.round_of(c).is_some()),
        (Form::Piece(half), _) => Form::Lump(half),
        (form, _) => form,
    };
    #[allow(clippy::cast_possible_truncation)] // render-space sizes
    match (form, thing.made) {
        (Form::Rod { radius, length, taper }, made) => {
            let mesh = match made {
                Made::Colour(c) => meshes.round_of(c)?,
                Made::Log(k) => meshes.log(usize::from(k))?,
                Made::Deadwood => meshes.deadwood?,
            };
            let d = (2.0 * radius) as f32;
            Some((
                mesh,
                Frame64::new(thing.centre, thing.turn * lying),
                Vec3::new(d, length as f32, d),
                taper as f32,
            ))
        }
        (Form::Block(half), Made::Colour(c)) => Some((
            meshes.board_of(c)?,
            Frame64::new(thing.centre, thing.turn),
            (half * 2.0).as_vec3(),
            1.0,
        )),
        (Form::Lump(half), _) => Some((
            meshes.stone?,
            Frame64::new(thing.centre, thing.turn),
            (half * 2.0).as_vec3(),
            1.0,
        )),
        _ => None,
    }
}

/// Draws `things` at `origin` into `scene`. Returns what has no mesh of its own (a rod or block
/// of a material that is not wood), for the caller to draw plainly.
pub fn draw(scene: &mut Scene, meshes: &PartMeshes, origin: DVec3, things: &[Thing]) -> Vec<Thing> {
    let mut rest = Vec::new();
    for thing in things {
        let Some((mesh, frame, scale, taper)) = placed(thing, meshes) else {
            rest.push(*thing);
            continue;
        };
        let material = if Some(mesh) == meshes.stone {
            let Made::Colour([r, g, b]) = thing.made else {
                rest.push(*thing);
                continue;
            };
            Material::dielectric([r, g, b].map(|c| f32::from(c) / 255.0), 0.9)
        } else {
            let (tone, seed) = tone_seed(thing.key);
            Material::part(tone, seed, taper)
        };
        scene.add(
            mesh,
            Instance {
                frame: Frame64::new(origin + frame.position, frame.rotation),
                scale,
                material: material.aged(thing.age.0, thing.age.1),
                heat: None,
            },
        );
    }
    rest
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rod's prism is told from a stone's ellipsoid; tones differ from part to part.
    #[test]
    fn a_rod_s_hull_is_told_by_its_rings() {
        let prism: Vec<DVec3> = (0..32)
            .map(|k| {
                let a = f64::from(k % 16) * std::f64::consts::TAU / 16.0;
                let x = if k < 16 { -0.8 } else { 0.8 };
                DVec3::new(x, 0.05 * a.cos(), 0.05 * a.sin())
            })
            .collect();
        let (radius, length) = rod_of(&prism).expect("a rod");
        assert!((radius - 0.05).abs() < 1e-9 && (length - 1.6).abs() < 1e-9);
        let lump: Vec<DVec3> = (0..62)
            .map(|k| {
                let a = f64::from(k) * 0.7;
                DVec3::new(0.1 * a.cos(), 0.05 * a.sin(), 0.08 * (a * 0.3).sin())
            })
            .collect();
        assert!(rod_of(&lump).is_none());
        let tones: Vec<f32> = (0..50).map(|k| tone_seed(k).0).collect();
        let mean = tones.iter().sum::<f32>() / 50.0;
        assert!((0.35..0.65).contains(&mean), "{mean}");
        assert!(tones.iter().all(|t| (0.0..=1.0).contains(t)));
        assert_ne!(tone_seed(1), tone_seed(2));
    }
}
