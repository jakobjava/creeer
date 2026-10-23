"""Relief on the engineer's garments (G1.3f): the shapes `detail.py` carves into the smooth pieces
`outfit.py` builds, so they read as worn workwear up close. Large and medium folds belong in the
geometry, fine detail in the surface (the polycount and real-time clothing practice ADR-0026's
detail survey found); here are the sleeves' and legs' hang, the folds and the seams, moved along
the normals, and the raised panels sewn on (a yoke, a pocket, a patch, a cuff).

Everything is in the ragdoll's frame (standing on the soles; Blender's axes, facing −y), placed
by its joints (`bones.json`), after the pieces are posed and lifted.
"""

import json
import math
import pathlib

import bmesh
import numpy as np
from mathutils import Vector

HERE = pathlib.Path(__file__).resolve().parent
BONES = json.loads((HERE / "bones.json").read_text(encoding="utf-8"))


def joint(name):
    """Joint `name`'s place (m, Blender's axes)."""
    x, y, z = next(j for j in BONES["joints"] if j["name"] == name)["at"]
    return np.array((x, -z, y))


def segment(name):
    """Segment `name`'s centre (m, Blender's axes)."""
    x, y, z = next(s for s in BONES["segments"] if s["name"] == name)["centre"]
    return np.array((x, -z, y))


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def arrays(mesh):
    """`mesh`'s vertex positions and normals, (n, 3) each."""
    n = len(mesh.vertices)
    co, no = np.empty(n * 3), np.empty(n * 3)
    mesh.vertices.foreach_get("co", co)
    mesh.vertex_normals.foreach_get("vector", no)
    return co.reshape(n, 3), no.reshape(n, 3)


def displace(mesh, d):
    """Each vertex moved `d` (m, one a vertex) along its normal."""
    co, no = arrays(mesh)
    mesh.vertices.foreach_set("co", (co + no * d[:, None]).ravel())
    mesh.update()


def weights(mesh, names):
    """The sum of `mesh`'s segment weights `names` (`s.<segment>`), one a vertex."""
    out = np.zeros(len(mesh.vertices))
    for name in names:
        a = mesh.attributes.get(f"s.{name}")
        if a is not None:
            w = np.empty(len(mesh.vertices))
            a.data.foreach_get("value", w)
            out += w
    return out


class Limb:
    """A limb as its chain of joints (a leg's hip, knee and ankle): where a point lies along it
    (m from the first joint), round it (rad: 0 on its outer side, π/2 in front, ±π on its inner
    side) and how far off its axis (m). `side` is +1 for the left (+x), −1 for the right."""

    def __init__(self, names, side, drop=0.0):
        self.points = np.array([joint(n) if isinstance(n, str) else n for n in names]) - np.array((0.0, 0.0, drop))
        self.side = side
        d = np.diff(self.points, axis=0)
        self.lengths = np.linalg.norm(d, axis=1)
        self.dirs = d / self.lengths[:, None]
        self.starts = np.concatenate(([0.0], np.cumsum(self.lengths)[:-1]))
        self.frames = []
        for u in self.dirs:
            out = np.array((side, 0.0, 0.0))
            out = out - out.dot(u) * u
            out /= np.linalg.norm(out)
            front = np.cross(u, out)
            if front[1] > 0:  # in front is −y
                front = -front
            self.frames.append((out, front))

    def coords(self, co, units=False):
        """(along, round, off) for the points `co` (n, 3); and, given `units`, the directions
        out from the axis to them."""
        best = np.full(len(co), np.inf)
        along, theta, off = np.zeros(len(co)), np.zeros(len(co)), np.zeros(len(co))
        unit = np.zeros((len(co), 3))
        for k, (a, u, length) in enumerate(zip(self.points, self.dirs, self.lengths)):
            s = np.clip((co - a) @ u, 0.0, length)
            radial = co - (a + s[:, None] * u)
            dist = np.linalg.norm(radial, axis=1)
            closer = dist < best
            out, front = self.frames[k]
            best = np.where(closer, dist, best)
            along = np.where(closer, self.starts[k] + s, along)
            theta = np.where(closer, np.arctan2(radial @ front, radial @ out), theta)
            off = np.where(closer, dist, off)
            unit = np.where(closer[:, None], radial / np.maximum(dist, 1e-9)[:, None], unit)
        return (along, theta, off, unit) if units else (along, theta, off)

    def length(self):
        return float(self.lengths.sum())

    def at(self, t):
        """The point on the axis `t` (m) along it, and the axis's direction there."""
        k = int(np.searchsorted(self.starts, t, side="right") - 1)
        k = min(max(k, 0), len(self.dirs) - 1)
        return self.points[k] + self.dirs[k] * (t - self.starts[k]), self.dirs[k]


def girth(t, off, mask, step=0.01):
    """The mean distance off the axis along the limb (m), smoothed over `step` bins: the arc
    round it measured on that, the quilting's diamonds keep their size where it narrows."""
    on = mask > 0.5
    if not on.any():
        return off
    bins = np.floor(t[on] / step).astype(int)
    lo = bins.min()
    sums = np.bincount(bins - lo, off[on])
    counts = np.bincount(bins - lo)
    centres = (np.arange(len(sums)) + lo + 0.5) * step
    keep = counts > 0
    return np.interp(t, centres[keep], sums[keep] / counts[keep])


def angle_from(theta, at):
    """How far round (rad, 0–π) `theta` lies from `at`."""
    return np.abs((theta - at + np.pi) % (2 * np.pi) - np.pi)


def seams(mesh, limb, mask, at=(0.0, math.pi), depth=0.0015, width=0.004):
    """Seams down the limb (rad round it: the outseam and the inseam), each a groove `depth`
    (m) deep, `width` (m) each side of its stitches."""
    co, _ = arrays(mesh)
    t, theta, off = limb.coords(co)
    near = np.min([angle_from(theta, s) for s in at], axis=0) * girth(t, off, mask)
    # From below the crotch: there the two legs' inseams met in one crease.
    below = smoothstep(0.12, 0.18, t)
    displace(mesh, -mask * below * depth * (1.0 - smoothstep(0.0, width, near)))


def hang(mesh, limb, mask, slope, start=0.0, columns=48, passes=6):
    """A sleeve or a trouser leg hung from what holds it up: going down the limb from its widest
    point it comes back in by at most `slope` (m per m), in each of `columns` columns round
    it, so it falls past the hollows below (behind the knee, the calf's foot, the elbow's
    inside) instead of following the body in (the user: "perfectly morphed around the body").
    The growth is smoothed over the mesh (`passes`), so the columns blend; it begins `start` (m)
    down the limb (below a shoulder's cap, which holds the sleeve)."""
    co, _ = arrays(mesh)
    t, theta, off, unit = limb.coords(co, units=True)
    mask = mask * smoothstep(start, start + 0.06, t)
    column = np.floor((theta + np.pi) / (2 * np.pi) * columns).astype(int) % columns
    # Hung from below `start` only: the crotch bridges the legs, and hung from there the legs
    # grew into each other.
    on = (mask > 0.5) & (t > start)
    grow = np.zeros(len(co))
    for c in range(columns):
        idx = np.nonzero(on & (column == c))[0]
        if len(idx) == 0:
            continue
        idx = idx[np.argsort(t[idx])]
        reach = np.maximum.accumulate(off[idx] + slope * t[idx])
        grow[idx] = np.maximum(0.0, reach - slope * t[idx] - off[idx])
    grow = smoothed(mesh, grow * np.clip(mask, 0, 1), passes)
    mesh.vertices.foreach_set("co", (co + unit * grow[:, None]).ravel())
    mesh.update()


def smoothed(mesh, values, passes):
    """`values` (one a vertex) averaged with their neighbours' `passes` times."""
    e = np.empty(len(mesh.edges) * 2, dtype=np.int64)
    mesh.edges.foreach_get("vertices", e)
    a, b = e[0::2], e[1::2]
    n = len(values)
    degree = np.maximum(np.bincount(a, minlength=n) + np.bincount(b, minlength=n), 1)
    for _ in range(passes):
        values = 0.5 * values + 0.5 * (np.bincount(a, values[b], n) + np.bincount(b, values[a], n)) / degree
    return values


def rings(mesh, limb, start, end, wavelength, amplitude, mask, window=None, seed=0.0, wobble=1.0):
    """Folds across the limb between `start` and `end` (m along it), `wavelength` apart: cloth
    stacked where it gathers (a sleeve above its cuff, a leg above the boot) or creased where a
    joint bends (the inner elbow, the back of the knee: a `window` = (rad round, half width)
    limits them to that side). They wander round the limb, so they zigzag rather than ring it
    (Bridgman's zigzag and half-lock folds)."""
    co, _ = arrays(mesh)
    t, theta, _ = limb.coords(co)
    edge = 0.25 * (end - start)
    envelope = smoothstep(start, start + edge, t) * (1.0 - smoothstep(end - edge, end, t))
    phase = 2 * np.pi * (t - start) / wavelength + wobble * (0.9 * np.sin(theta + seed) + 0.45 * np.sin(2 * theta + 1.7 + seed) + 0.3 * np.sin(3 * theta + 0.4 * seed))
    # Smooth throughout: a ridge rising from a flat valley with a kink came out faceted.
    ridge = (0.5 + 0.5 * np.sin(phase)) ** 2
    if window is not None:
        at, half = window
        envelope = envelope * (1.0 - smoothstep(0.5 * half, half, angle_from(theta, at)))
    displace(mesh, mask * envelope * amplitude * (ridge - 0.375))


def span(mesh, mask, most=0.015, frame=None):
    """Cloth stretched over the hollows of a region (`mask`, one a vertex): each vertex there
    moved out along its normal to the region's convex hull, by at most `most` (m), as a stiff
    canvas spans from the neck to the shoulder points instead of following the body down
    between them. The hull is of the vertices where `frame` (one a vertex; `mask` by default)
    holds: leaving out the neck's edge, it does not cap the opening there."""
    from mathutils.bvhtree import BVHTree

    co, no = arrays(mesh)
    idx = np.nonzero(mask > 0.05)[0]
    frame = mask if frame is None else frame
    hull = np.nonzero(frame > 0.5)[0]
    if len(hull) < 4:
        return
    bm = bmesh.new()
    verts = [bm.verts.new(co[i]) for i in hull]
    bmesh.ops.convex_hull(bm, input=verts)
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    tree = BVHTree.FromBMesh(bm)
    bm.free()
    d = np.zeros(len(co))
    for i in idx:
        hit, _, _, dist = tree.ray_cast(Vector(co[i]) + Vector(no[i]) * 1e-5, Vector(no[i]), most)
        if hit is not None:
            d[i] = dist
    displace(mesh, d * np.clip(mask, 0, 1))


def blouse(mesh, belt, mask, amplitude=0.006, folds=15):
    """A jacket gathered under its belt (m, the belt's middle height): it puffs out above and
    below the belt and falls in soft pipe folds round the waist."""
    co, _ = arrays(mesh)
    z = co[:, 2]
    theta = np.arctan2(co[:, 0], -co[:, 1])
    above = smoothstep(belt + 0.022, belt + 0.04, z) * (1.0 - smoothstep(belt + 0.05, belt + 0.15, z))
    below = smoothstep(belt - 0.022, belt - 0.035, z) * (1.0 - smoothstep(belt - 0.045, belt - 0.07, z))
    pipes = 0.55 + 0.45 * np.cos(folds * theta + 1.1 * np.sin(3 * theta + 0.4))
    displace(mesh, mask * amplitude * (above + 0.6 * below) * pipes)


def layer(bm, names):
    """A function giving a bmesh vertex's summed segment weights `names` (`s.<segment>`)."""
    layers = [bm.verts.layers.float.get(f"s.{n}") for n in names]
    layers = [x for x in layers if x is not None]
    return lambda v: sum(v[x] for x in layers)


SNAP = 0.002


def key(co):
    """A point as a dictionary key, to 1 µm."""
    return tuple(round(c * 1e6) for c in co)


def panel(obj, planes, height, near, material=None, groove=0.0015):
    """A piece sewn on: the surface of `obj` on the inner side of every plane (co, no: what `no`
    points to is outside) among the vertices where `near(bm)(v)` holds (it is given the bmesh,
    to read weights: a cuff's plane must not cut the hip beside it), cut out along the planes
    and raised `height` (m) along its normals, a step at its outline; given in `material` (an
    index into the mesh's materials). With no height it lies flush, its seam a groove `groove`
    (m) deep along the outline: a step's wall, where a plane crosses the surface aslant, shaded
    in a saw-tooth. Returns the panel's faces' count."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    # The layer first: adding one drops the faces already taken from `bm`.
    part = bm.faces.layers.int.get("part") or bm.faces.layers.int.new("part")
    test = near(bm)
    planes = [(Vector(c), Vector(n).normalized()) for c, n in planes]
    for co, no in planes:
        faces = [f for f in bm.faces if all(test(v) for v in f.verts)]
        # Vertices within 2 mm of the plane moved onto it (along it, so across the surface):
        # cut beside them, the faces left slivers that shaded as smudges along the outline.
        for v in {v for f in faces for v in f.verts}:
            d = (v.co - co).dot(no)
            if abs(d) < SNAP:
                v.co -= no * d
        geom = list({v for f in faces for v in f.verts}) + list({e for f in faces for e in f.edges}) + faces
        bmesh.ops.bisect_plane(bm, geom=geom, plane_co=co, plane_no=no, dist=1e-5)
    bm.normal_update()
    region = []
    for f in bm.faces:
        c = f.calc_center_median()
        if all(test(v) for v in f.verts) and all((c - co).dot(no) < 0 for co, no in planes):
            region.append(f)
    if not region:
        bm.free()
        return 0
    # A piece of its own cloth, toned apart (`surface.py`; the bake tones each part).
    number = 1 + max(f[part] for f in bm.faces)
    for f in region:
        f[part] = number
    if height <= 0.0:
        inner = set(region)
        rim = {v for f in region for v in f.verts if any(g not in inner for g in v.link_faces)}
        for v in rim:
            v.co -= v.normal * groove
        if material is not None:
            for f in region:
                f.material_index = material
        bm.to_mesh(obj.data)
        bm.free()
        obj.data.update()
        return len(region)
    # Raised along the surface's smooth normals from before the cut: the slivers a cut leaves
    # have no sound normal of their own, and an outline raised along them zigzags.
    smooth = {key(v.co): v.normal.copy() for f in region for v in f.verts}
    made = bmesh.ops.extrude_face_region(bm, geom=region)
    # The copy is raised; the faces it was copied from stay behind unless removed.
    bmesh.ops.delete(bm, geom=region, context="FACES")
    verts = [g for g in made["geom"] if isinstance(g, bmesh.types.BMVert)]
    raised = set(verts)
    tops = set(f for f in bm.faces if all(v in raised for v in f.verts))
    for v in verts:
        v.co += smooth[key(v.co)] * height
    # The step's two edges shaded hard: smooth across a right angle, it smeared.
    walls = {f for f in made["geom"] if isinstance(f, bmesh.types.BMFace) and f not in tops}
    for f in walls:
        for e in f.edges:
            if any(g not in walls for g in e.link_faces):
                e.smooth = False
    if material is not None:
        for f in bm.faces:
            if any(v in raised for v in f.verts):
                f.material_index = material
    count = len(tops)
    bm.to_mesh(obj.data)
    bm.free()
    obj.data.update()
    return count


def material_index(obj, mat):
    """`mat`'s index among `obj`'s materials, added if it is not there."""
    names = [m.name for m in obj.data.materials]
    if mat.name not in names:
        obj.data.materials.append(mat)
        names.append(mat.name)
    return names.index(mat.name)


def over(obj, under, gap):
    """`obj` pushed out to `gap` (m) off `under` wherever it sank into it."""
    import bpy

    wrap = obj.modifiers.new("over", "SHRINKWRAP")
    wrap.target, wrap.wrap_method, wrap.wrap_mode, wrap.offset = under, "NEAREST_SURFACEPOINT", "OUTSIDE", gap
    with bpy.context.temp_override(object=obj, active_object=obj):
        bpy.ops.object.modifier_apply(modifier="over")


def subdivide(obj, levels=1):
    """`obj` subdivided (Catmull–Clark, its shape kept): fine enough for the relief."""
    import bpy

    mod = obj.modifiers.new("relief", "SUBSURF")
    mod.levels = mod.render_levels = levels
    mod.boundary_smooth = "PRESERVE_CORNERS"
    with bpy.context.temp_override(object=obj, active_object=obj):
        bpy.ops.object.modifier_apply(modifier="relief")
