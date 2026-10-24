//! Building a hull (`hull.rs`): incremental construction of its triangles, merged into one
//! face per plane.

use super::hull::{Edge, Face, HullError};
use creeer_math::glam::DVec3;
use std::collections::{HashMap, HashSet};

/// The hull's triangles (corners counter-clockwise seen from outside), by incremental
/// construction.
pub(super) fn triangles(points: &[DVec3], eps: f64) -> Result<Vec<[usize; 3]>, HullError> {
    let start = tetrahedron(points, eps).ok_or(HullError::Flat)?;
    let inside = start.iter().map(|&i| points[i]).sum::<DVec3>() * 0.25;
    let plane = |t: &[usize; 3]| {
        let (a, b, c) = (points[t[0]], points[t[1]], points[t[2]]);
        let n = (b - a).cross(c - a).normalize();
        (n, n.dot(a))
    };
    let mut faces: Vec<[usize; 3]> = Vec::new();
    for skip in 0..4 {
        let mut t = [0; 3];
        let mut k = 0;
        for (j, &i) in start.iter().enumerate() {
            if j != skip {
                t[k] = i;
                k += 1;
            }
        }
        let (n, d) = plane(&t);
        if n.dot(inside) > d {
            t.swap(1, 2);
        }
        faces.push(t);
    }
    let mut planes: Vec<(DVec3, f64)> = faces.iter().map(plane).collect();
    for (i, &p) in points.iter().enumerate() {
        if start.contains(&i) {
            continue;
        }
        let visible = visible_region(&faces, &planes, p, eps);
        if visible.is_empty() {
            continue;
        }
        let seen: HashSet<(usize, usize)> = visible
            .iter()
            .flat_map(|&f| {
                let t = faces[f];
                [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]
            })
            .collect();
        // The horizon: edges of the visible faces whose other face is not visible.
        let mut fan = Vec::new();
        for &f in &visible {
            let t = faces[f];
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                if !seen.contains(&(b, a)) {
                    fan.push([a, b, i]);
                }
            }
        }
        let mut kept = 0;
        for f in 0..faces.len() {
            if visible.binary_search(&f).is_err() {
                faces[kept] = faces[f];
                planes[kept] = planes[f];
                kept += 1;
            }
        }
        faces.truncate(kept);
        planes.truncate(kept);
        for t in fan {
            planes.push(plane(&t));
            faces.push(t);
        }
    }
    Ok(faces)
}

/// The faces `p` sees (more than `eps` above their planes) that join the one it sees best
/// across edges, sorted. Every face it sees is not enough: with nearly coplanar points (a
/// member's twelve-sided end rings, G2.8's builders) a face within `eps` lay between faces it
/// saw, the horizon was no single loop, and the hull came out with edges of one face only.
fn visible_region(faces: &[[usize; 3]], planes: &[(DVec3, f64)], p: DVec3, eps: f64) -> Vec<usize> {
    let above = |f: usize| planes[f].0.dot(p) - planes[f].1;
    let Some(best) = (0..faces.len()).max_by(|&a, &b| above(a).total_cmp(&above(b))) else {
        return Vec::new();
    };
    if above(best) <= eps {
        return Vec::new();
    }
    let mut owner = HashMap::new();
    for (f, t) in faces.iter().enumerate() {
        for k in 0..3 {
            owner.insert((t[k], t[(k + 1) % 3]), f);
        }
    }
    let mut seen = vec![false; faces.len()];
    seen[best] = true;
    let mut stack = vec![best];
    let mut region = Vec::new();
    while let Some(f) = stack.pop() {
        region.push(f);
        let t = faces[f];
        for k in 0..3 {
            if let Some(&g) = owner.get(&(t[(k + 1) % 3], t[k]))
                && !seen[g]
                && above(g) > eps
            {
                seen[g] = true;
                stack.push(g);
            }
        }
    }
    region.sort_unstable();
    region
}

/// Four points of `points` spanning a tetrahedron as large as can be found cheaply.
fn tetrahedron(points: &[DVec3], eps: f64) -> Option<[usize; 4]> {
    let furthest = |score: &dyn Fn(DVec3) -> f64| {
        (0..points.len()).max_by(|&a, &b| score(points[a]).total_cmp(&score(points[b])))
    };
    let a = furthest(&|p| -p.x)?;
    let b = furthest(&|p| (p - points[a]).length())?;
    let line = (points[b] - points[a]).normalize_or_zero();
    let off_line = |p: DVec3| (p - points[a]).reject_from_normalized(line).length();
    let c = furthest(&off_line)?;
    let normal = line.cross(points[c] - points[a]).normalize_or_zero();
    let d = furthest(&|p| normal.dot(p - points[a]).abs())?;
    let span = (points[b] - points[a]).length();
    (span > eps && off_line(points[c]) > eps && normal.dot(points[d] - points[a]).abs() > eps)
        .then_some([a, b, c, d])
}

/// The triangles merged into faces, one per plane, without corners on straight edges; the
/// corners in use, renumbered.
pub(super) fn merge(
    points: &[DVec3],
    triangles: &[[usize; 3]],
    eps: f64,
) -> (Vec<DVec3>, Vec<Face>) {
    // Group triangles that share an edge and lie in one plane (union–find).
    let mut parent: Vec<usize> = (0..triangles.len()).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let normal = |t: &[usize; 3]| {
        let (a, b, c) = (points[t[0]], points[t[1]], points[t[2]]);
        (b - a).cross(c - a).normalize()
    };
    let mut owner = HashMap::new();
    for (f, t) in triangles.iter().enumerate() {
        for k in 0..3 {
            owner.insert((t[k], t[(k + 1) % 3]), f);
        }
    }
    for (f, t) in triangles.iter().enumerate() {
        let n = normal(t);
        for k in 0..3 {
            let Some(&g) = owner.get(&(t[(k + 1) % 3], t[k])) else {
                continue;
            };
            let in_plane = triangles[g]
                .iter()
                .all(|&v| n.dot(points[v] - points[t[0]]).abs() <= eps);
            if in_plane {
                let (r, s) = (root(&mut parent, f), root(&mut parent, g));
                parent[r.max(s)] = r.min(s);
            }
        }
    }
    // Each group's boundary: its directed edges whose reverse is not in the group.
    let mut groups: Vec<usize> = (0..triangles.len())
        .map(|f| root(&mut parent, f))
        .collect::<Vec<_>>();
    let roots: Vec<usize> = {
        let mut r = groups.clone();
        r.sort_unstable();
        r.dedup();
        r
    };
    let mut faces = Vec::with_capacity(roots.len());
    for &r in &roots {
        let members: Vec<usize> = (0..triangles.len()).filter(|&f| groups[f] == r).collect();
        let edges: HashSet<(usize, usize)> = members
            .iter()
            .flat_map(|&f| {
                let t = triangles[f];
                [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]
            })
            .collect();
        let mut next = HashMap::new();
        for &(a, b) in &edges {
            if !edges.contains(&(b, a)) {
                next.insert(a, b);
            }
        }
        // Start at the smallest corner, so the ring does not depend on hashing.
        let first = *next.keys().min().expect("a face has a boundary");
        let mut ring = vec![first];
        let mut at = next[&first];
        while at != first {
            ring.push(at);
            at = next[&at];
        }
        // Area-weighted normal of the whole polygon (Newell), for the plane.
        let n = members
            .iter()
            .map(|&f| {
                let t = triangles[f];
                (points[t[1]] - points[t[0]]).cross(points[t[2]] - points[t[0]])
            })
            .sum::<DVec3>()
            .normalize();
        faces.push((n, ring));
    }
    groups.clear();
    // Corners on a straight edge of their faces lie on a straight edge of the hull: drop them
    // (from both faces along that edge).
    let straight = |ring: &[usize], k: usize| {
        let len = ring.len();
        let (p, v, q) = (
            points[ring[(k + len - 1) % len]],
            points[ring[k]],
            points[ring[(k + 1) % len]],
        );
        let line = (q - p).normalize();
        (v - p).reject_from_normalized(line).length() <= eps
    };
    // A corner stays if any face it is on bends there: judged face by face, a corner nearly on
    // a straight edge was dropped from one face and kept in the other, their edges no longer
    // met, and building a member's hull panicked (G2.8's builders).
    let mut bends = vec![false; points.len()];
    for (_, ring) in &faces {
        for k in 0..ring.len() {
            bends[ring[k]] |= !straight(ring, k);
        }
    }
    let mut used = vec![false; points.len()];
    let mut out = Vec::with_capacity(faces.len());
    for (n, ring) in faces {
        let kept: Vec<usize> = ring.into_iter().filter(|&v| bends[v]).collect();
        for &v in &kept {
            used[v] = true;
        }
        out.push((n, kept));
    }
    let mut index = vec![usize::MAX; points.len()];
    let mut vertices = Vec::new();
    for (i, &p) in points.iter().enumerate() {
        if used[i] {
            index[i] = vertices.len();
            vertices.push(p);
        }
    }
    let faces = out
        .into_iter()
        .map(|(normal, ring)| {
            let ring: Vec<usize> = ring.into_iter().map(|v| index[v]).collect();
            let offset = ring.iter().map(|&v| normal.dot(vertices[v])).sum::<f64>()
                / f64::from(u32::try_from(ring.len()).expect("a few corners"));
            Face {
                normal,
                offset,
                ring,
                // Once the hull is centred (`Hull::centre`).
                rectangle: [DVec3::ZERO; 4],
            }
        })
        .collect();
    (vertices, faces)
}

/// The edges of `faces`, each once.
pub(super) fn edges(faces: &[Face]) -> Vec<Edge> {
    let mut by_ends = HashMap::new();
    for (f, face) in faces.iter().enumerate() {
        let len = face.ring.len();
        for k in 0..len {
            by_ends.insert((face.ring[k], face.ring[(k + 1) % len]), f);
        }
    }
    let mut edges = Vec::new();
    for (f, face) in faces.iter().enumerate() {
        let len = face.ring.len();
        for k in 0..len {
            let (a, b) = (face.ring[k], face.ring[(k + 1) % len]);
            if a < b {
                let other = by_ends[&(b, a)];
                edges.push(Edge {
                    ends: [a, b],
                    faces: [f, other],
                });
            }
        }
    }
    edges
}
