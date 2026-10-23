"""Trims for the engineer's outfit (G1.3f): the small hard parts and straps `detail.py` adds to
the garments, made as meshes: steel buttons, eyelets, hooks and buckles; laces; leather straps
round a limb. Each is light enough to keep whole at the close-up level (a 1 cm button needs no
more than 12 sides: its facets stray 0.4 mm, under §7's 1 mm), and is weighted after the
garment it sits on (`skinning.complete_weights`).
"""

import math

import bmesh
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree


def basis(normal, up=None):
    """A frame whose z is `normal` and whose y leans to `up` (world z by default)."""
    z = Vector(normal).normalized()
    up = Vector(up) if up is not None else Vector((0, 0, 1))
    if abs(up.normalized().dot(z)) > 0.99:
        up = Vector((0, 1, 0))
    x = up.cross(z).normalized()
    y = z.cross(x)
    return x, y, z


def ring(bm, centre, normal, major, minor, segments=12, sides=6, up=None):
    """A torus: `major` (m) round its middle, its tube `minor` (m) thick, about `normal`."""
    x, y, z = basis(normal, up)
    centre = Vector(centre)
    rows = []
    for i in range(segments):
        a = 2 * math.pi * i / segments
        out = x * math.cos(a) + y * math.sin(a)
        row = []
        for j in range(sides):
            b = 2 * math.pi * j / sides
            row.append(bm.verts.new(centre + out * (major + minor * math.cos(b)) + z * (minor * math.sin(b))))
        rows.append(row)
    for i in range(segments):
        a, b = rows[i], rows[(i + 1) % segments]
        for j in range(sides):
            bm.faces.new((a[j], b[j], b[(j + 1) % sides], a[(j + 1) % sides]))


def dome(bm, centre, normal, radius, height, segments=12, rings=3):
    """A button: a low dome `radius` (m) across, `height` (m) high, on its flat back."""
    x, y, z = basis(normal)
    centre = Vector(centre)
    tip = bm.verts.new(centre + z * height)
    rows = []
    for k in range(1, rings + 1):
        polar = (math.pi / 2) * k / rings
        r, h = radius * math.sin(polar), height * math.cos(polar)
        rows.append([bm.verts.new(centre + (x * math.cos(a) + y * math.sin(a)) * r + z * h) for a in (2 * math.pi * i / segments for i in range(segments))])
    back = bm.verts.new(centre)
    for i in range(segments):
        j = (i + 1) % segments
        bm.faces.new((tip, rows[0][i], rows[0][j]))
        for a, b in zip(rows, rows[1:]):
            bm.faces.new((a[i], b[i], b[j], a[j]))
        bm.faces.new((rows[-1][j], rows[-1][i], back))


def tube(bm, points, radius, sides=6, closed=False):
    """A round tube through `points` (a lace, a buckle's frame, a prong), its ends capped."""
    points = [Vector(p) for p in points]
    n = len(points)
    rows = []
    prev = None
    for i, p in enumerate(points):
        if closed:
            t = points[(i + 1) % n] - points[i - 1]
        else:
            t = points[min(i + 1, n - 1)] - points[max(i - 1, 0)]
        t.normalize()
        side = prev if prev is not None else (Vector((0, 0, 1)) if abs(t.z) < 0.9 else Vector((1, 0, 0)))
        a = (side - t * side.dot(t)).normalized()
        b = t.cross(a)
        prev = a
        rows.append([bm.verts.new(p + (a * math.cos(2 * math.pi * j / sides) + b * math.sin(2 * math.pi * j / sides)) * radius) for j in range(sides)])
    for i in range(n if closed else n - 1):
        a, b = rows[i], rows[(i + 1) % n]
        for j in range(sides):
            bm.faces.new((a[j], b[j], b[(j + 1) % sides], a[(j + 1) % sides]))
    if not closed:
        bm.faces.new(list(reversed(rows[0])))
        bm.faces.new(rows[-1])


def band(bm, points, outward, across, width, thickness, closed=True):
    """A strap through `points`, lying on the surface whose outward normals are `outward` (one a
    point), `width` (m) wide along `across` (one direction a point) and `thickness` (m) thick:
    its section a rectangle."""
    # Its section's corners. Two more, halfway across its broad faces, drew nothing on a band
    # flat across: a third of every strap's triangles.
    corners = [(0.5, -0.5), (0.5, 0.5), (-0.5, 0.5), (-0.5, -0.5)]
    sides = len(corners)
    rows = []
    for p, n, w in zip(points, outward, across):
        p, n, w = Vector(p), Vector(n).normalized(), Vector(w).normalized()
        middle = p + n * (thickness / 2)
        rows.append([bm.verts.new(middle + w * (u * width) + n * (v * thickness)) for u, v in corners])
    count = len(rows)
    for i in range(count if closed else count - 1):
        a, b = rows[i], rows[(i + 1) % count]
        for j in range(sides):
            bm.faces.new((a[j], a[(j + 1) % sides], b[(j + 1) % sides], b[j]))
    if not closed:
        bm.faces.new(rows[0])
        bm.faces.new(list(reversed(rows[-1])))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)


def buckle(bm, centre, normal, along, width, height, tube_radius=0.0025, prong=True, fine=False):
    """A steel buckle: a rounded rectangular frame `width` (m) along the strap (`along`) by
    `height` (m) across it, standing on the strap, with its prong; `fine`, its corners in
    smaller steps and its sides split (a large buckle: §7's facets)."""
    z = Vector(normal).normalized()
    x = (Vector(along) - z * Vector(along).dot(z)).normalized()
    y = z.cross(x)
    c = Vector(centre) + z * tube_radius
    r = min(width, height) * 0.25
    # Fine, each corner in 22.5° steps and each side in three: on the belt's 5 cm buckle, with
    # fewer, its tube's faces strayed past §7's millimetre where a long side met a corner.
    steps, splits = (5, (1 / 3, 2 / 3)) if fine else (3, ())
    loop = []
    for cx, cy, a0 in ((1, 1, 0), (-1, 1, 90), (-1, -1, 180), (1, -1, 270)):
        corner = c + x * (cx * (width / 2 - r)) + y * (cy * (height / 2 - r))
        arc = [corner + (x * math.cos(a) + y * math.sin(a)) * r for a in (math.radians(a0 + 90 / (steps - 1) * k) for k in range(steps))]
        if loop:
            loop.extend(loop[-1].lerp(arc[0], f) for f in splits)
        loop.extend(arc)
    loop.extend(loop[-1].lerp(loop[0], f) for f in splits)
    tube(bm, loop, tube_radius, sides=6, closed=True)
    if prong:
        tube(bm, [c - x * (width / 2), c + x * (width * 0.15) + z * tube_radius], tube_radius * 0.6, sides=5)


class Surface:
    """A mesh object's surface to place trims on: the nearest point and its normal, and where a
    ray meets it."""

    def __init__(self, obj):
        mesh = obj.data
        self.tree = BVHTree.FromPolygons([v.co.copy() for v in mesh.vertices], [tuple(p.vertices) for p in mesh.polygons])

    def nearest(self, co):
        hit, normal, _, _ = self.tree.find_nearest(Vector(co))
        return hit, normal

    def cast(self, origin, direction):
        hit, normal, _, _ = self.tree.ray_cast(Vector(origin), Vector(direction).normalized())
        return hit, normal


def loop_round(obj, limb, t, mask, gap=0.002, bins=32, width=0.016):
    """Points round the garment `obj` where it crosses the limb `t` (m) along (`relief.Limb`),
    among its vertices where `mask` (one a vertex) is over a half (the limb's own, not the
    trunk or the other leg beside it): in each of `bins` directions round the axis its
    outermost point across the strap's `width` (m), smoothed round, `gap` (m) out; with the
    outward directions, the axis, and the smoothed outline (m off the axis, one a direction). A
    strap's path."""
    n = len(obj.data.vertices)
    co = np.empty(n * 3)
    obj.data.vertices.foreach_get("co", co)
    co = co.reshape(n, 3)
    along, theta, off = limb.coords(co)
    near = (np.abs(along - t) < width / 2 + 0.002) & (mask > 0.5)
    centre, axis = limb.at(t)
    k = np.floor((theta[near] + np.pi) / (2 * np.pi) * bins).astype(int) % bins
    best = np.zeros(bins)
    for b, r in zip(k, off[near]):
        best[b] = max(best[b], r)
    if (best == 0).any():  # gaps: the neighbours' mean
        filled = best.copy()
        for b in np.nonzero(best == 0)[0]:
            filled[b] = 0.5 * (best[(b - 1) % bins] + best[(b + 1) % bins])
        best = filled
    # Smoothed round the loop: the leg's outline jumps between directions, and a strap kinked
    # there. Where the smooth path runs below the cloth (over a knee patch's step), the cloth is
    # drawn in under it (`cinch_round`), as a buckled strap gathers it: kept above every bump,
    # the strap kinked over them (facets 1.1 mm off their curve) and stood off the leg's sides.
    for _ in range(3):
        best = np.convolve(np.concatenate((best[-2:], best, best[:2])), np.ones(5) / 5, mode="valid")
    out, front = limb.frames[min(int(np.searchsorted(limb.starts, t, side="right") - 1), len(limb.frames) - 1)]
    points, normals = [], []
    for b in range(bins):
        a = -np.pi + (b + 0.5) * 2 * np.pi / bins
        d = out * math.cos(a) + front * math.sin(a)
        points.append(Vector(centre + d * (best[b] + gap)))
        normals.append(Vector(d))
    return points, normals, Vector(axis), best


def cinch_round(obj, limb, t, mask, radius, width, fall=0.012):
    """`obj`'s cloth round the limb at `t` (m along), where `mask` is over a half, drawn in within
    `radius` (one a direction round it, as `loop_round` gives it) across a strap's `width` (m),
    easing out over `fall` (m) above and below: a buckled strap gathers what it goes round."""
    n = len(obj.data.vertices)
    co = np.empty(n * 3)
    obj.data.vertices.foreach_get("co", co)
    co = co.reshape(n, 3)
    along, theta, off = limb.coords(co)
    bins = len(radius)
    pos = (theta + np.pi) / (2 * np.pi) * bins - 0.5
    lo = np.floor(pos).astype(int)
    f = pos - lo
    r = radius[lo % bins] * (1 - f) + radius[(lo + 1) % bins] * f
    x = np.clip((np.abs(along - t) - width / 2) / fall, 0.0, 1.0)
    hold = (1.0 - x * x * (3 - 2 * x)) * (mask > 0.5)
    pull = np.maximum(off - r, 0.0) * hold
    centre, axis = (np.array(v) for v in limb.at(t))
    d = co - centre
    radial = d - np.outer(d @ axis, axis)
    radial /= np.maximum(np.linalg.norm(radial, axis=1), 1e-9)[:, None]
    co -= radial * pull[:, None]
    obj.data.vertices.foreach_set("co", co.ravel())
    obj.data.update()


def sharpen(mesh, degrees, keep=False):
    """`mesh`'s edges shaded hard where its faces meet at more than `degrees`; the rest smooth
    (or, given `keep`, as they were)."""
    limit = math.cos(math.radians(degrees))
    faces = {}
    for p in mesh.polygons:
        p.use_smooth = True
        for key in p.edge_keys:
            faces.setdefault(key, []).append(p.normal.copy())
    sharp = mesh.attributes.get("sharp_edge") or mesh.attributes.new("sharp_edge", "BOOLEAN", "EDGE")
    for e in mesh.edges:
        ns = faces.get(tuple(sorted(e.vertices)), [])
        hard = len(ns) == 2 and ns[0].dot(ns[1]) < limit
        sharp.data[e.index].value = hard or (keep and sharp.data[e.index].value)


def place(pieces, piece, name, bm, material, host, only=None):
    """`bm` made an object of `piece` in `material`, weighted later as its `host` (an object) is
    on the segments `only` (all by default)."""
    obj = pieces.mesh(piece, name, bm, Matrix.Identity(4), material)
    obj["host"] = host.name
    if only:
        obj["only"] = list(only)
    return obj
