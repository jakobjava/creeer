"""Garments as offsets of the fitted body (G1.3f; ADR-0026): a copy of the skin, cut along
planes where the garment ends, pushed out along the normals by the cloth's thickness, and given
a thickness at its edges. Each copy keeps the body's limb weights, so it is posed with it.
"""

import math

import bmesh
import bpy
import numpy as np
from mathutils import Vector

LIMBS = ("w_arm.L", "w_arm.R", "w_forearm.L", "w_forearm.R", "w_leg.L", "w_leg.R", "w_foot.L", "w_foot.R")
# Where garments hang rather than cling (m: fading in from the first height to the second, out
# from the third to the fourth): the hips, from below the buttocks' fold to above the seat (the
# user, 2026-10-21: not accentuated), and the trunk, for a jacket that falls from the chest.
SEAT = (0.6, 0.74, 1.02, 1.14)
# A model's garments get their edges' thickness after their levels are made (`models.py`):
# decimated with it, a garment's thin rim came out ragged. Set, `make` only records it.
DEFER_EDGES = [False]
TRUNK = (0.6, 0.74, 1.4, 1.5)


class Cloth:
    """A copy of `src`'s mesh being cut into a garment."""

    def __init__(self, src):
        self.bm = bmesh.new()
        self.bm.from_mesh(src.data)
        self.layers = {n: self.bm.verts.layers.float.get(n) for n in LIMBS}

    def weight(self, v, name):
        layer = self.layers[name]
        return v[layer] if layer is not None else 0.0

    def arm(self, v):
        return self.weight(v, "w_arm.L") + self.weight(v, "w_arm.R")

    def drop(self, test):
        """Removes the vertices where `test(v)` holds."""
        bmesh.ops.delete(self.bm, geom=[v for v in self.bm.verts if test(v)], context="VERTS")
        return self

    def cut(self, co, no, where=lambda v: True):
        """Cuts along the plane through `co` and removes what lies on the side `no` points
        to, among the vertices where `where(v)` holds."""
        co, no = Vector(co), Vector(no).normalized()
        faces = [f for f in self.bm.faces if all(where(v) for v in f.verts)]
        edges = list({e for f in faces for e in f.edges})
        verts = list({v for f in faces for v in f.verts})
        bmesh.ops.bisect_plane(self.bm, geom=verts + edges + faces, plane_co=co, plane_no=no, dist=1e-5)
        gone = [v for v in self.bm.verts if where(v) and (v.co - co).dot(no) > 1e-5]
        bmesh.ops.delete(self.bm, geom=gone, context="VERTS")
        return self

    def hull(self, voxel=0.0025):
        """Each side's part replaced by its convex hull: a boot's shape round a foot, the toes
        and the arch gone; remeshed evenly (`voxel`, m: a hull's faces are long slivers, which
        subdivided and smoothed into streaks), each vertex weighted as the nearest it had.
        Open where it was cut (the hull's cap there is cut off)."""
        top = max(v.co.z for v in self.bm.verts)
        out = bmesh.new()
        # Every weight the vertices carry, the limbs' and the segments' (`body.weigh_segments`).
        source = dict(self.bm.verts.layers.float.items())
        layers = {n: out.verts.layers.float.new(n) for n in source}
        for side in (1, -1):
            verts = []
            for v in self.bm.verts:
                if v.co.x * side > 0:
                    u = out.verts.new(v.co)
                    for n, layer in layers.items():
                        u[layer] = v[source[n]]
                    verts.append(u)
            r = bmesh.ops.convex_hull(out, input=verts)
            unused = {g for g in r["geom_interior"] + r["geom_unused"] if isinstance(g, bmesh.types.BMVert)}
            bmesh.ops.delete(out, geom=list(unused), context="VERTS")
        self.bm.free()
        self.bm = remeshed(out, voxel)
        out.free()
        self.layers = {n: self.bm.verts.layers.float.get(n) for n in LIMBS}
        return self.cut((0, 0, top - 0.004), (0, 0, 1))

    def keep(self, test):
        """Keeps only the vertices where `test(v)` holds."""
        return self.drop(lambda v: not test(v))

    def push(self, by):
        """Moves every vertex out along its normal by `by(v)` (m)."""
        self.bm.normal_update()
        moves = [(v, v.normal.copy() * by(v)) for v in self.bm.verts]
        for v, d in moves:
            v.co += d
        return self

    def make(self, scene, name, out, over=None, gap=0.0, passes=0, hang=None, edge=0.004, subdivide=0):
        """The garment: pushed `out` (m) off the skin, smoothed by `passes` of `soften` (and,
        given `hang` = (heights, passes), that many more, far stronger, between those heights,
        arms aside: there the cloth hangs, spanning the cleft, the crotch and the chest's
        relief instead of following them), and pushed back out to `gap` (m) off whatever it
        is worn `over` where it sank into it; so cloth spans the hollows instead of showing
        them. It is `edge` (m) thick where it ends; its open edges' points are kept as
        `rim`."""
        self.push(lambda v: out)
        mesh = bpy.data.meshes.new(name)
        self.bm.to_mesh(mesh)
        self.bm.free()
        obj = bpy.data.objects.new(name, mesh)
        scene.collection.objects.link(obj)
        if subdivide:
            sub = obj.modifiers.new("subdivide", "SUBSURF")
            sub.levels = sub.render_levels = subdivide
            with bpy.context.temp_override(object=obj, active_object=obj):
                bpy.ops.object.modifier_apply(modifier="subdivide")
        soften(mesh, passes)
        if hang:
            (a, b, c, d), more = hang
            region = np.array([
                smoothstep(a, b, v.co.z) * (1.0 - smoothstep(c, d, v.co.z)) * max(0.0, 1.0 - 4.0 * w)
                for v, w in zip(mesh.vertices, arm_weights(mesh))
            ])
            soften(mesh, more, shrink=-0.501, where=region)
        if over is not None:
            wrap = obj.modifiers.new("over", "SHRINKWRAP")
            wrap.target, wrap.wrap_method, wrap.wrap_mode = over, "NEAREST_SURFACEPOINT", "OUTSIDE"
            wrap.offset = gap
            with bpy.context.temp_override(object=obj, active_object=obj):
                bpy.ops.object.modifier_apply(modifier="over")
        even_rims(mesh)
        boundary = {i for k, n in _edge_uses(mesh).items() if n == 1 for i in k}
        self.rim = [mesh.vertices[i].co.copy() for i in boundary]
        if edge and DEFER_EDGES[0]:
            thickness = mesh.attributes.get("edge") or mesh.attributes.new("edge", "FLOAT", "POINT")
            thickness.data.foreach_set("value", [edge] * len(mesh.vertices))
        elif edge:
            solid = obj.modifiers.new("edge", "SOLIDIFY")
            solid.thickness = edge
            solid.offset = -1.0
            solid.thickness_clamp = 0.5  # no spikes where bisected slivers meet
            with bpy.context.temp_override(object=obj, active_object=obj):
                bpy.ops.object.modifier_apply(modifier="edge")
        for p in mesh.polygons:
            p.use_smooth = True
        return obj


def even_rims(mesh, passes=8):
    """Each open edge (a cuff, a hem) made an even line: its points averaged along it, which the
    cut left straight but the push off the skin beneath made ragged."""
    neighbours = {}
    for (a, b), n in _edge_uses(mesh).items():
        if n == 1:
            neighbours.setdefault(a, []).append(b)
            neighbours.setdefault(b, []).append(a)
    if not neighbours:
        return
    co = {i: mesh.vertices[i].co.copy() for i in neighbours}
    for _ in range(passes):
        co = {i: c.lerp(sum((co[j] for j in neighbours[i]), Vector()) / len(neighbours[i]), 0.5) for i, c in co.items()}
    for i, c in co.items():
        mesh.vertices[i].co = c
    mesh.update()


def remeshed(bm, voxel):
    """`bm` (closed) remeshed by voxels `voxel` (m) across, Blender's voxel remesh; each new
    vertex given the float layers of `bm`'s nearest."""
    from mathutils.kdtree import KDTree

    mesh = bpy.data.meshes.new("remesh")
    bm.to_mesh(mesh)
    obj = bpy.data.objects.new("remesh", mesh)
    bpy.context.scene.collection.objects.link(obj)
    mod = obj.modifiers.new("remesh", "REMESH")
    mod.mode, mod.voxel_size = "VOXEL", voxel
    with bpy.context.temp_override(object=obj, active_object=obj):
        bpy.ops.object.modifier_apply(modifier="remesh")
    out = bmesh.new()
    out.from_mesh(mesh)
    bpy.data.objects.remove(obj)
    bpy.data.meshes.remove(mesh)
    tree = KDTree(len(bm.verts))
    for v in bm.verts:
        tree.insert(v.co, v.index)
    tree.balance()
    bm.verts.ensure_lookup_table()
    source = dict(bm.verts.layers.float.items())
    layers = {n: out.verts.layers.float.get(n) or out.verts.layers.float.new(n) for n in source}
    for v in out.verts:
        near = bm.verts[tree.find(v.co)[1]]
        for n, layer in layers.items():
            v[layer] = near[source[n]]
    return out


def soften(mesh, passes, grow=0.5, shrink=-0.505, where=None):
    """Taubin's smoothing (1995): each pass averages every vertex towards its neighbours and
    then a little away, so small features fade while large shapes keep their size (plain
    averaging would shrink the limbs to nothing). With these factors the pass band ends near
    k = 1/grow + 1/shrink = 0.02: on his 1 cm edges, reliefs narrower than about 30 cm (a
    muscle's) fade, a limb's girth stays. The open edges stay where they were cut; `where`
    (one weight per vertex) limits it to a region."""
    if not passes:
        return
    n = len(mesh.vertices)
    co = np.empty(n * 3)
    mesh.vertices.foreach_get("co", co)
    co = co.reshape(n, 3)
    e = np.empty(len(mesh.edges) * 2, dtype=np.int64)
    mesh.edges.foreach_get("vertices", e)
    a, b = e[0::2], e[1::2]
    degree = np.bincount(a, minlength=n) + np.bincount(b, minlength=n)
    free = np.ones(n)
    free[[i for k, uses in _edge_uses(mesh).items() if uses == 1 for i in k]] = 0.0
    if where is not None:
        free *= where
    for _ in range(passes):
        for f in (grow, shrink):
            total = np.stack(
                [np.bincount(a, co[b, j], n) + np.bincount(b, co[a, j], n) for j in range(3)], axis=1
            )
            co += (f * free / np.maximum(degree, 1))[:, None] * (total - degree[:, None] * co)
    mesh.vertices.foreach_set("co", co.ravel())
    mesh.update()


def fall(mesh, lo, hi, where, slope=0.3, columns=120):
    """Cloth on the trunk between the heights `lo` and `hi` hangs from what holds it up (the
    chest, the shoulder blades, the seat): going down from a high point it comes back in by at
    most `slope` (m per m), round the trunk's axis in `columns` columns, so it falls past the
    hollows below them instead of following the body in. `where` (one weight per vertex) keeps
    the arms out of it."""
    n = len(mesh.vertices)
    co = np.empty(n * 3)
    mesh.vertices.foreach_get("co", co)
    co = co.reshape(n, 3)
    r = np.hypot(co[:, 0], co[:, 1])
    column = np.floor((np.arctan2(co[:, 1], co[:, 0]) + np.pi) / (2 * np.pi) * columns).astype(int)
    on = np.nonzero((co[:, 2] > lo) & (co[:, 2] < hi) & (where > 0.5))[0]
    out = r.copy()
    for c in np.unique(column[on]):
        idx = on[column[on] == c]
        idx = idx[np.argsort(-co[idx, 2])]
        reach = np.maximum.accumulate(r[idx] - slope * (hi - co[idx, 2]))
        out[idx] = np.maximum(r[idx], reach + slope * (hi - co[idx, 2]))
    grow = np.where(r > 1e-9, out / np.maximum(r, 1e-9), 1.0)
    co[:, 0] *= grow
    co[:, 1] *= grow
    mesh.vertices.foreach_set("co", co.ravel())
    mesh.update()


def arm_weights(mesh):
    """Each vertex's weight on either arm."""
    out = [0.0] * len(mesh.vertices)
    for name in ("w_arm.L", "w_arm.R"):
        a = mesh.attributes.get(name)
        if a is not None:
            out = [x + d.value for x, d in zip(out, a.data)]
    return out


def _edge_uses(mesh):
    """How many faces use each edge of `mesh`."""
    uses = {}
    for p in mesh.polygons:
        for k in p.edge_keys:
            uses[k] = uses.get(k, 0) + 1
    return uses


def smoothstep(a, b, x):
    t = min(max((x - a) / (b - a), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def hairline(d):
    """How far up (m, from the head's centre) the hair begins in the direction `d` (Blender's
    axes, facing −y): high at the forehead, above the ears at the sides, low at the nape."""
    theta = math.atan2(d.x, -d.y)
    return 0.00125 + 0.0575 * math.cos(theta) - 0.00375 * math.cos(2 * theta)


def hair(scene, skin, centre, name="hair"):
    """Short hair: the scalp 4 mm out above the hairline, sunk under the skin below it (a soft
    edge), the rest dropped."""
    cloth = Cloth(skin)
    cloth.keep(lambda v: v.co.z > centre.z - 0.09 and cloth.arm(v) < 0.01)

    def lift(v):
        line = hairline(v.co - centre)
        return 0.007 * smoothstep(line - 0.012, line + 0.006, v.co.z - centre.z) - 0.003

    cloth.push(lift)
    sunk = {v for v in cloth.bm.verts if v.co.z - centre.z < hairline(v.co - centre) - 0.012}
    cloth.drop(lambda v: v in sunk)
    return cloth.make(scene, name, 0.0, edge=0.0)


def collar(scene, name, rim, skin, height=0.035, gap=0.008, thickness=0.005):
    """A stand collar: a band of cloth rising `height` (m) from the neckline `rim` (a closed
    loop, in order) and leaning in to stand `gap` (m) off the neck, `thickness` (m) thick."""
    middle = sum(rim, Vector()) / len(rim)
    neck = [v.co for v in skin.data.vertices]
    bm = bmesh.new()
    rings = []
    for p in rim:
        d = Vector((p.x - middle.x, p.y - middle.y, 0.0)).normalized()
        top = p.z + height
        # The neck's skin at the collar's top in this direction (within 10°), if it is there.
        near = [c for c in neck if abs(c.z - top) < 0.006 and Vector((c.x - middle.x, c.y - middle.y, 0.0)).normalized().dot(d) > 0.985]
        reach = max((Vector((c.x - middle.x, c.y - middle.y, 0.0)).dot(d) for c in near), default=None)
        r = (p - middle).xy.length
        r_top = min(r, reach + gap) if reach is not None else r
        ring = [p.copy()]
        for t in (0.5, 1.0):
            q = middle.xy + d.xy * (r + (r_top - r) * t * t)
            ring.append(Vector((q.x, q.y, p.z + height * t)))
        rings.append([bm.verts.new(c) for c in ring])
    for a, b in zip(rings, rings[1:] + rings[:1]):
        for k in range(2):
            bm.faces.new((a[k], b[k], b[k + 1], a[k + 1]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    scene.collection.objects.link(obj)
    for kind, settings in (("SUBSURF", {"levels": 2}), ("SOLIDIFY", {"thickness": thickness, "offset": 0.0})):
        mod = obj.modifiers.new(kind.lower(), kind)
        for k, v in settings.items():
            setattr(mod, k, v)
        with bpy.context.temp_override(object=obj, active_object=obj):
            bpy.ops.object.modifier_apply(modifier=mod.name)
    for poly in mesh.polygons:
        poly.use_smooth = True
    return obj


def pad(scene, name, src, centre, across, up, facing, limb, off=0.003, thick=0.008):
    """A pad on the garment `src` (posed): its surface on the `limb` (a weight's name) on the
    side `facing` of `centre`, cut to an octagon `across` by `up` (m, half sizes; across x, up
    z), `off` (m) off it and `thick` (m) thick."""
    cloth = Cloth(src)
    cloth.bm.normal_update()
    facing = Vector(facing)
    # The limb's outer surface on that side (not the far side, nor the trunk beside it), cut
    # generously: the octagon's planes give the edges.
    def near(v):
        d = v.co - centre
        on = cloth.weight(v, limb) > 0.5 and d.dot(facing) > 0 and v.normal.dot(facing) > -0.2
        return on and abs(d.x) < across + 0.03 and abs(d.z) < up + 0.03

    cloth.keep(near)
    for k in range(8):
        a = k * math.pi / 4
        reach = 1.0 / math.hypot(math.cos(a) / across, math.sin(a) / up)
        out = Vector((math.cos(a), 0.0, math.sin(a)))
        cloth.cut(centre + out * reach, out)
    obj = cloth.make(scene, name, off, edge=thick)
    obj["off"] = off
    return obj


def head_centre(skin, eyes):
    """The head's centre (as the hairline is measured from): 8 mm above the eyes, midway
    between the face and the back of the head."""
    eye = sum((v.co for e in eyes for v in e.data.vertices), Vector()) / sum(len(e.data.vertices) for e in eyes)
    head = [v.co for v in skin.data.vertices if eye.z - 0.01 < v.co.z < eye.z + 0.03]
    return Vector((0.0, (min(c.y for c in head) + max(c.y for c in head)) / 2, eye.z + 0.008))


def around(points):
    """`points` in order round their middle, seen from above."""
    middle = sum(points, Vector()) / len(points)
    return sorted(points, key=lambda c: math.atan2(c.x - middle.x, c.y - middle.y))


def smoothed(loop, reach=4):
    """The closed `loop` averaged over `reach` points each side: a rim cut through the mesh's
    faces, made a smooth line."""
    n = len(loop)
    return [sum((loop[(i + k) % n] for k in range(-reach, reach + 1)), Vector()) / (2 * reach + 1) for i in range(n)]


def ring(obj, z, test=lambda co: True, band=0.006):
    """The outermost points of `obj` near the height `z`, in order round their middle (every 5°)."""
    near = [v.co for v in obj.data.vertices if abs(v.co.z - z) < band and test(v.co)]
    middle = sum(near, Vector()) / len(near)
    bins = {}
    for c in near:
        k = round(math.degrees(math.atan2(c.x - middle.x, -(c.y - middle.y))) / 5)
        d = Vector((c.x - middle.x, c.y - middle.y)).length
        if d > bins.get(k, (0.0, None))[0]:
            bins[k] = (d, Vector((c.x, c.y, z)))
    return [bins[k][1] for k in sorted(bins)], middle
