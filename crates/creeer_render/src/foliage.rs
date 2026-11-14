//! Foliage's meshes (G2.11, ADR-0037; `art-style.md` §8: "masses of foliage in clumps, not
//! individual leaves; dark undersides from AO"): a clump, a lumpy ball of smaller lumps, for a
//! broadleaf's or a pine's crown; a tier, a conifer's whorl of branches drooping to their tips,
//! for a spruce; and a spire, a conifer's whole crown of tiers in one mesh, for one far off.
//! Each is a model (its vertices name a leaves token and carry their occlusion and tone, as a
//! Blender bake would: `art/bake.py`), smooth-shaded, in levels of detail chosen by their error
//! on screen. Each fits the cube of side 1 about the origin and is scaled to its crown.

use crate::mesh::MeshData;
use crate::primitives::sphere_error;
use crate::vertex::{MeshVertex, pack_detail, pack_material};
use creeer_math::glam::Vec3;
use std::f32::consts::{PI, TAU};

/// The segments around of each level, coarsest first.
pub const LEVELS: [u32; 6] = [8, 12, 16, 24, 32, 48];
/// The lumps on a clump: as many directions, spread evenly (a Fibonacci sphere).
const LUMPS: usize = 14;
/// A tier's branch tips around.
const TIPS: f32 = 11.0;
/// A spire's tiers.
const SPIRE_TIERS: usize = 6;

/// How far `x` is from `a` to `b` (0–1), smoothly.
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A clump's lumps' directions.
fn lump_directions() -> [Vec3; LUMPS] {
    std::array::from_fn(|k| {
        #[allow(clippy::cast_precision_loss)] // fourteen
        let (i, n) = (k as f32 + 0.5, LUMPS as f32);
        let y = 1.0 - 2.0 * i / n;
        let r = (1.0 - y * y).max(0.0).sqrt();
        let a = i * PI * (3.0 - 5f32.sqrt());
        Vec3::new(r * a.cos(), y, r * a.sin())
    })
}

/// A clump's surface out along unit `d`: its radius (of 0.5), the lump it is on (for its tone),
/// and how much it stands out (1 on a lump's crown, 0 in the creases between). Its lumps carry
/// smaller ones (twigs' tufts), so its outline is ragged, never a ball's.
fn clump_at(d: Vec3, lumps: &[Vec3; LUMPS]) -> (f32, usize, f32) {
    let (mut best, mut which) = (0.0f32, 0);
    let mut tufts = 0.0f32;
    for (k, c) in lumps.iter().enumerate() {
        let near = d.dot(*c);
        let bump = smooth(0.66, 1.0, near).powf(0.7);
        if bump > best {
            (best, which) = (bump, k);
        }
        // Three tufts about each lump's crown, turned by the lump.
        let side = c.cross(if c.y.abs() < 0.9 { Vec3::Y } else { Vec3::X }).normalize();
        for t in 0..3 {
            #[allow(clippy::cast_precision_loss)] // three
            let a = t as f32 * TAU / 3.0 + k as f32;
            let at = (*c * 0.86 + (side * a.cos() + c.cross(side) * a.sin()) * 0.5).normalize();
            tufts = tufts.max(smooth(0.93, 1.0, d.dot(at)));
        }
    }
    (0.5 * (0.7 + 0.24 * best + 0.06 * tufts), which, best)
}

/// A level of a surface `at(θ, φ) -> (position, occlusion, tone)` over a sphere's angles,
/// `segments` around and half as many rings, with normals across it and the leaves `token`.
fn swept(segments: u32, token: u16, at: impl Fn(f32, f32) -> (Vec3, f32, f32)) -> MeshData {
    let (n, rings) = (segments.max(3), (segments / 2).max(2));
    let mut mesh = MeshData::default();
    #[allow(clippy::cast_precision_loss)] // a few dozen
    let (fr, fs) = (rings as f32, n as f32);
    let e = 1e-3;
    for ring in 0..=rings {
        #[allow(clippy::cast_precision_loss)]
        let theta = PI * ring as f32 / fr;
        for s in 0..=n {
            #[allow(clippy::cast_precision_loss)]
            let phi = TAU * s as f32 / fs;
            let (p, occlusion, tone) = at(theta, phi);
            // The normal across the surface: its two tangents' cross (outwards, as the angles
            // run); at the poles, along the axis.
            let dt = at((theta + e).min(PI), phi).0 - at((theta - e).max(0.0), phi).0;
            let dp = at(theta, phi + e).0 - at(theta, phi - e).0;
            let mut normal = dt.cross(dp).normalize_or_zero();
            if normal.length_squared() < 0.5 || dp.length() < 1e-6 {
                normal = Vec3::new(0.0, if theta < PI / 2.0 { 1.0 } else { -1.0 }, 0.0);
            }
            mesh.vertices.push(MeshVertex {
                position: p.to_array(),
                normal: normal.to_array(),
                material: pack_material(token, occlusion, 0.0),
                // High in the air: no splash from the ground (`surface.wgsl`).
                detail: pack_detail(tone, 1.0),
                grain: [0.0; 2],
            });
        }
    }
    let stride = n + 1;
    for ring in 0..rings {
        for s in 0..n {
            let a = ring * stride + s;
            let (b, c, d) = (a + stride, a + stride + 1, a + 1);
            if ring != 0 {
                mesh.indices.extend_from_slice(&[a, b, d]);
            }
            if ring != rings - 1 {
                mesh.indices.extend_from_slice(&[d, b, c]);
            }
        }
    }
    mesh
}

/// The direction of angles `(θ, φ)`: θ from +Y, φ about it.
fn direction(theta: f32, phi: f32) -> Vec3 {
    Vec3::new(theta.sin() * phi.cos(), theta.cos(), -theta.sin() * phi.sin())
}

/// A clump at `segments` around, of the leaves `token`: lumps on a ball, darker in their creases
/// and underneath, each lump its own tone.
pub fn clump(segments: u32, token: u16) -> MeshData {
    let lumps = lump_directions();
    swept(segments, token, |theta, phi| {
        let d = direction(theta, phi);
        let (radius, which, out) = clump_at(d, &lumps);
        let under = 0.5 + 0.5 * d.y;
        let occlusion = (0.4 + 0.6 * under) * (0.5 + 0.5 * out);
        #[allow(clippy::cast_precision_loss)] // fourteen
        let tone = 0.3 + 0.4 * ((which * 7 % LUMPS) as f32 / LUMPS as f32);
        (d * radius, occlusion, tone)
    })
}

/// How far out a tier reaches along `phi` (0–1): its branches' tips, each pointed and of its
/// own length, and the hollows between.
fn tips(phi: f32) -> f32 {
    let s = phi.rem_euclid(TAU) * TIPS / TAU;
    let i = s.round();
    let f = s - i;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a tip's index
    let k = (i as u32) % (TIPS as u32);
    #[allow(clippy::cast_precision_loss)] // eleven
    let length = 0.75 + 0.25 * ((k * 7 + 3) % 11) as f32 / 10.0;
    (1.0 - 2.0 * f.abs()).max(0.0).powf(1.6) * length
}

/// A tier at `segments` around, of the leaves `token`: a whorl's branches as a skirt from the
/// stem (its top, at y 0.5) drooping to their tips (a ring at radius 0.5 near y −0.5), its
/// underside a shallower cone back to the stem, dark.
pub fn tier(segments: u32, token: u16) -> MeshData {
    // Along the profile: 0 at the top, ½ at the tips, 1 under the stem.
    swept(segments, token, |theta, phi| {
        let u = theta / PI;
        let tip = tips(phi);
        let reach = 0.5 * (0.72 + 0.28 * tip);
        let (r, y, occlusion) = if u <= 0.5 {
            let t = u / 0.5;
            // The top bows out a little, as branches spring from the stem and droop.
            let r = reach * (t + 0.15 * (PI * t).sin()).min(1.0);
            let y = 0.5 - t * (0.9 + 0.1 * tip);
            (r, y, 0.6 + 0.4 * t)
        } else {
            let t = (u - 0.5) / 0.5;
            let r = reach * (1.0 - t);
            let y = -0.4 - 0.1 * tip + t * 0.45;
            (r, y, 0.45 - 0.15 * t)
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a tip's index
        let branch = ((phi / TAU * TIPS + 0.5) as u32) % 5;
        #[allow(clippy::cast_precision_loss)]
        let tone = 0.35 + 0.075 * branch as f32;
        (Vec3::new(r * phi.cos(), y, -r * phi.sin()), occlusion, tone)
    })
}

/// A spire at `segments` around: [`SPIRE_TIERS`] tiers stacked to a point, each narrower than
/// the one below, as a far spruce's crown (radius 0.5 at its base, from y −0.5 to 0.5).
pub fn spire(segments: u32, token: u16) -> MeshData {
    let mut mesh = MeshData::default();
    #[allow(clippy::cast_precision_loss)] // six
    let n = SPIRE_TIERS as f32;
    for k in 0..SPIRE_TIERS {
        #[allow(clippy::cast_precision_loss)]
        let (lo, hi) = (k as f32 / n, ((k as f32 + 1.6) / n).min(1.0));
        let width = 1.0 - lo * 0.9;
        let one = tier(segments, token);
        let first = u32::try_from(mesh.vertices.len()).expect("a small mesh");
        for mut v in one.vertices {
            let p = Vec3::from(v.position);
            let y = -0.5 + (p.y + 0.5) * (hi - lo) + lo;
            v.position = [p.x * width, y, p.z * width];
            let normal = Vec3::from(v.normal) * Vec3::new(1.0 / width, 1.0 / (hi - lo), 1.0 / width);
            v.normal = normal.normalize().to_array();
            mesh.vertices.push(v);
        }
        mesh.indices.extend(one.indices.iter().map(|i| i + first));
    }
    mesh
}

/// A foliage mesh's levels of detail (coarsest first), each with its silhouette error as a
/// share of its radius: a sphere's at its segments, and the lumps or tips a coarse level cannot
/// show (an estimate, conservative: `LEVELS` start at 8 around).
pub fn levels(make: impl Fn(u32) -> MeshData, features: f32) -> Vec<(MeshData, f32)> {
    LEVELS
        .iter()
        .map(|&n| {
            #[allow(clippy::cast_precision_loss)] // dozens
            let lost = 0.2 * (1.0 - n as f32 / (4.0 * features)).clamp(0.0, 1.0);
            (make(n), 2.0 * sphere_error(n) + lost)
        })
        .collect()
}

/// The clump's levels, of `token`.
pub fn clump_levels(token: u16) -> Vec<(MeshData, f32)> {
    levels(|n| clump(n, token), 4.0)
}

/// The tier's levels, of `token`.
pub fn tier_levels(token: u16) -> Vec<(MeshData, f32)> {
    levels(|n| tier(n, token), TIPS)
}

/// The spire's levels, of `token`.
pub fn spire_levels(token: u16) -> Vec<(MeshData, f32)> {
    levels(|n| spire(n, token), TIPS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every level is within the unit cube, its normals unit and turned outwards (away from the
    /// axis), its vertices naming the token; finer levels err less.
    #[test]
    fn foliage_meshes_are_closed_lit_and_within_their_cube() {
        for levels in [clump_levels(7), tier_levels(7), spire_levels(7)] {
            let mut last = f32::MAX;
            for (mesh, error) in &levels {
                assert!(*error < last);
                last = *error;
                assert!(!mesh.indices.is_empty());
                for v in &mesh.vertices {
                    let (p, n) = (Vec3::from(v.position), Vec3::from(v.normal));
                    assert!(p.abs().max_element() <= 0.5 + 1e-4, "{p}");
                    assert!((n.length() - 1.0).abs() < 1e-3, "{n}");
                    assert_eq!(v.material & 0xffff, 7);
                }
            }
        }
        // A clump's creases are darker than its lumps' crowns, its underside than its top.
        let mesh = clump(32, 7);
        let occ = |v: &MeshVertex| (v.material >> 16) & 0xff;
        let top = mesh.vertices.iter().max_by_key(|v| (v.position[1] * 1e4) as i32);
        let bottom = mesh.vertices.iter().min_by_key(|v| (v.position[1] * 1e4) as i32);
        assert!(occ(top.expect("a top")) > occ(bottom.expect("a bottom")));
    }
}
