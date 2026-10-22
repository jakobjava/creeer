"""The boots' shape (G1.3f; `boots.py`): each boot grown round its own foot, as a last is
made to a foot, then given a work boot's room. The blockout's boot was the foot's convex hull:
low at the toes, high at the ankle, a wedge on a flat sole (the user: "triangles on flip
flops"). Ellipsoids merged into a boot were tried next: a clown's shoe, its toe a ball apart
from its instep, and the big toe through its side (ADR-0026, Measured and rejected).

- **The upper**: the foot and ankle closed into a solid, merged with a toe box (an ellipsoid
  over the toes, with room above them) and a shaft (an elliptic cone round the shin, the
  coverall's leg bloused into it), all grown 11 mm outward for the leather and padding:
  the foot is inside by its making, the toes lost in the box. Voxel remeshing merges and cleans;
  Taubin smoothing rounds the joins. Cut level with the sole's top and at the shaft's top.
  Balls round each of the skin's points were tried: every toe showed as a lump.
- **The sole**: round the upper's foot 4 mm out (the welt's ledge): rubber lugs round its edge,
  its tread lifted under the arch and at the toe, a leather welt on top.

In a foot's frame: metres ahead of the ankle, out to its side, up from the ground.
"""

import math

import bmesh
import bpy
from mathutils import Matrix, Vector

import garments as G
import relief as R

UP = Vector((0, 0, 1))
# The upper (m): its leather and padding round the foot; the toe box (centre ahead of the ankle,
# out, up; radii) before that; the shaft from its foot to its top, round the shin there with
# this much room for the coverall's leg, narrowing to its foot by this share.
LEATHER = 0.011
TOE_BOX = ((0.15, 0.0, 0.045), (0.06, 0.058, 0.04))
SHAFT = (0.08, 0.245, 0.016, 0.88)
VOXEL = 0.0025
SOLE_TOP = 0.03
# The sole's section (m up, m in from the outline) from the tread up: lugs to 1.2 cm, the
# rubber to 2.45 cm, the welt to 3 cm; every other step of the lugs in by `LUG`; the welt's
# rows from `WELT_FROM`. The tread lifted under the arch (inner side, outer side) between the
# heel block's front and the ball (m ahead of the ankle), and at the toe from `SPRING` on.
SECTION = ((0.0, 0.003), (0.003, 0.0), (0.012, 0.0), (0.0135, 0.0012), (0.0245, 0.0), (0.0255, 0.0), (0.0295, 0.0), (0.030, 0.006))
WELT_FROM = 4
LUG = 0.003
ARCH = ((0.012, 0.007), (0.012, 0.105))
SPRING = (0.17, 0.012)


def frame(tag):
    """A foot's ankle and its direction ahead (level)."""
    ankle = Vector(R.joint(f"ankle.{tag}"))
    centre = Vector(R.segment(f"foot.{tag}"))
    ahead = centre - ankle
    ahead.z = 0.0
    return ankle, ahead.normalized()


def placing(tag, side):
    """The foot's frame as a matrix: x ahead, y out to its side, z up, on the ground below the
    ankle."""
    ankle, ahead = frame(tag)
    out = Vector((-ahead.y, ahead.x, 0)) * side
    return Matrix((ahead, out, UP)).transposed().to_4x4(), Vector((ankle.x, ankle.y, 0.0))


def solid_foot(bm, skin, side, top):
    """The skin's leg on `side` below `top` (m) added to `bm`, cut level there and capped."""
    co, _ = R.arrays(skin.data)
    keep = (side * co[:, 0] > 0.02) & (co[:, 2] < top + 0.03)
    one = bmesh.new()
    one.from_mesh(skin.data)
    one.verts.ensure_lookup_table()
    bmesh.ops.delete(one, geom=[v for v in one.verts if not keep[v.index]], context="VERTS")
    bmesh.ops.bisect_plane(one, geom=one.verts[:] + one.edges[:] + one.faces[:], plane_co=(0, 0, top), plane_no=(0, 0, 1), dist=1e-5)
    bmesh.ops.delete(one, geom=[v for v in one.verts if v.co.z > top + 1e-5], context="VERTS")
    bmesh.ops.holes_fill(one, edges=one.edges[:], sides=0)
    mesh = bpy.data.meshes.new("foot")
    one.to_mesh(mesh)
    one.free()
    bm.from_mesh(mesh)
    bpy.data.meshes.remove(mesh)


def leg_ring(skin, tag, lo, hi):
    """The skin's shin between the heights `lo` and `hi`: its middle and half extents (m, x and
    y)."""
    co, _ = R.arrays(skin.data)
    on = (R.weights(skin.data, (f"shank.{tag}",)) > 0.5) & (co[:, 2] > lo) & (co[:, 2] < hi)
    near = co[on]
    lo_, hi_ = near[:, :2].min(axis=0), near[:, :2].max(axis=0)
    return (lo_ + hi_) / 2, (hi_ - lo_) / 2


def upper(scene, skin, tag, side):
    """The boot's upper: one smooth surface round the foot, open at the shaft's top and at the
    sole."""
    turn, origin = placing(tag, side)
    z0, z1, spare, taper = SHAFT
    bm = bmesh.new()
    solid_foot(bm, skin, side, z1)
    centre, radii = TOE_BOX
    made = bmesh.ops.create_uvsphere(bm, u_segments=32, v_segments=16, radius=1.0)
    bmesh.ops.transform(bm, matrix=Matrix.Translation(origin) @ turn @ Matrix.Translation(centre) @ Matrix.Diagonal((*radii, 1.0)), verts=made["verts"])
    middle, half = leg_ring(skin, tag, z1 - 0.05, z1 + 0.01)
    ru, rv = half + spare
    made = bmesh.ops.create_cone(bm, cap_ends=True, segments=48, radius1=taper, radius2=1.0, depth=1.0)
    shaft = Matrix.Translation((middle[0], middle[1], (z0 + z1) / 2 + 0.01)) @ Matrix.Diagonal((ru, rv, z1 - z0 + 0.02, 1.0))
    bmesh.ops.transform(bm, matrix=shaft, verts=made["verts"])
    solid = G.remeshed(bm, VOXEL)
    bm.free()
    solid.normal_update()
    for v in solid.verts:
        v.co += v.normal * LEATHER
    grown = G.remeshed(solid, VOXEL)
    solid.free()
    mesh = bpy.data.meshes.new(f"boot.{tag}")
    grown.to_mesh(mesh)
    grown.free()
    G.soften(mesh, 30)
    bm = bmesh.new()
    bm.from_mesh(mesh)
    for co_, no in (((0, 0, SOLE_TOP), (0, 0, -1)), ((0, 0, z1), (0, 0, 1))):
        bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], plane_co=co_, plane_no=no, dist=1e-5)
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if (v.co - Vector(co_)).dot(Vector(no)) > 1e-5], context="VERTS")
    bm.to_mesh(mesh)
    bm.free()
    # Weighted afresh from the skin (`skinning.complete_weights`): what it carried from the
    # foot's points, the toe box's and the shaft's points lacked.
    for name in [n for n in mesh.attributes.keys() if n.startswith(("s.", "w_"))]:
        mesh.attributes.remove(mesh.attributes[name])
    obj = bpy.data.objects.new(f"boot.{tag}", mesh)
    scene.collection.objects.link(obj)
    return obj


# The boots' padded collars (`boots.collar`): a tube 6.5 mm thick, its middle 3 mm out from
# the shaft's rim and 2 mm below its top.
COLLAR = (0.0065, 0.003, 0.002)


def tuck(coverall, boots, top, over=0.06, bins=48):
    """The coverall's legs bloused into the boots: inside the shaft and its padded collar (2 mm
    within the collar's inner side) up to 2 mm above the collar, then easing out over it into
    the blousing over the last `over` (m), round the shaft's middle in `bins` directions. A
    shrinkwrap pulled only what was outside in, a funnel within a few millimetres that came
    through the leather; drawn in to the shaft's rim alone, the legs came 5.6 mm through the
    collar."""
    import math

    import numpy as np

    co, _ = R.arrays(coverall.data)
    bo, _ = R.arrays(boots.data)
    for side in (1, -1):
        rim = bo[(side * bo[:, 0] > 0) & (bo[:, 2] > top - 0.02)]
        middle = rim[:, :2].mean(axis=0)
        angle = lambda p: (np.floor((np.arctan2(p[:, 1] - middle[1], p[:, 0] - middle[0]) + math.pi) / (2 * math.pi) * bins).astype(int)) % bins  # noqa: E731
        reach = np.zeros(bins)
        np.maximum.at(reach, angle(rim), np.linalg.norm(rim[:, :2] - middle, axis=1))
        found = np.nonzero(reach > 0)[0]
        reach = np.interp(np.arange(bins), found, reach[found], period=bins)
        thick, out, down = COLLAR
        lip = top - down + thick + 0.002
        legs = (side * co[:, 0] > 0.02) & (co[:, 2] < lip + over)
        d = co[legs, :2] - middle
        r = np.linalg.norm(d, axis=1)
        z = co[legs, 2]
        inside, outside = out - thick - 0.002, out + thick + 0.0035
        target = reach[angle(co[legs])] + inside + (outside - inside) * R.smoothstep(lip, lip + 0.012, z)
        pull = 1.0 - R.smoothstep(lip, lip + over, z)
        new = np.where(z < lip, np.minimum(r, target), r + (target - r) * pull)
        co[legs, :2] = middle + d * (new / np.maximum(r, 1e-9))[:, None]
    coverall.data.vertices.foreach_set("co", co.ravel())
    coverall.data.update()


def outline_of(obj, spacing=0.008):
    """The upper's foot (its open edge level with the sole's top), 4 mm out, every `spacing` m
    round it, in order."""
    uses = {}
    for p in obj.data.polygons:
        for key in p.edge_keys:
            uses[key] = uses.get(key, 0) + 1
    vs = obj.data.vertices
    nexts = {}
    for (i, j), n in uses.items():
        if n == 1 and vs[i].co.z < SOLE_TOP + 0.002 and vs[j].co.z < SOLE_TOP + 0.002:
            nexts.setdefault(i, []).append(j)
            nexts.setdefault(j, []).append(i)
    # Walked round in order (sorted by its angle round the middle, its points zigzagged and
    # the outline came out many times its length).
    start = min(nexts)
    loop, prev = [start], None
    while True:
        here = loop[-1]
        step = next((k for k in nexts[here] if k != prev), None)
        if step is None or step == start:
            break
        prev = here
        loop.append(step)
    rim = [vs[i].co.copy() for i in loop]
    middle = sum(rim, Vector()) / len(rim)
    rim = [Vector((c.x, c.y, 0)) + Vector((c.x - middle.x, c.y - middle.y, 0)).normalized() * 0.004 for c in rim]
    length = sum((rim[(i + 1) % len(rim)] - rim[i]).length for i in range(len(rim)))
    count = max(16, 2 * round(length / spacing / 2))
    out, i, travelled = [], 0, 0.0
    for k in range(count):
        target = k * length / count
        while travelled + (rim[(i + 1) % len(rim)] - rim[i]).length < target:
            travelled += (rim[(i + 1) % len(rim)] - rim[i]).length
            i += 1
        seg = rim[(i + 1) % len(rim)] - rim[i]
        out.append(rim[i] + seg * ((target - travelled) / max(seg.length, 1e-9)))
    return out


def lift(ahead_of_ankle, inner, tip):
    """How far the tread is lifted (m) at a point of the outline: under the arch between the
    heel block and the ball (more on the inner side), and towards the sprung toe."""
    (lift_in, lift_out), (heel, ball) = ARCH
    a = lift_in if inner else lift_out
    rise = float(R.smoothstep(heel - 0.006, heel + 0.012, ahead_of_ankle) * (1 - R.smoothstep(ball - 0.03, ball, ahead_of_ankle)))
    start, most = SPRING
    toe = most * max(0.0, (ahead_of_ankle - start) / max(tip - start, 1e-6)) ** 2
    return max(a * rise, toe)


def sole(bm, outline, tag, side, rubber, leather):
    """The sole round the closed `outline` (points on the ground, in order round it)."""
    ankle, ahead = frame(tag)
    middle = sum(outline, Vector()) / len(outline)
    d = [(c - Vector((ankle.x, ankle.y, 0))).dot(ahead) for c in outline]
    tip = max(d)
    out_dir = Vector((-ahead.y, ahead.x, 0)) * side
    rows = []
    for k, c in enumerate(outline):
        out = Vector((c.x - middle.x, c.y - middle.y, 0)).normalized()
        up = lift(d[k], (c - middle).dot(out_dir) < 0, tip)
        row = []
        for r, (z, inset) in enumerate(SECTION):
            lug = LUG if (k % 2 and r <= 2) else 0.0
            row.append(bm.verts.new(Vector((c.x, c.y, up + z * (SOLE_TOP - up) / SOLE_TOP)) - out * (inset + lug)))
        rows.append(row)
    n = len(rows)
    for k in range(n):
        a, b = rows[k], rows[(k + 1) % n]
        for r in range(len(SECTION) - 1):
            f = bm.faces.new((a[r], b[r], b[r + 1], a[r + 1]))
            f.material_index = leather if r >= WELT_FROM else rubber
    # The tread closed; the welt's top left open, under the boot (filled, it only made slivers).
    ring = [row[0] for row in rows]
    edges = [bm.edges.get((ring[k], ring[(k + 1) % n])) for k in range(n)]
    made = bmesh.ops.triangle_fill(bm, use_beauty=True, use_dissolve=False, edges=edges)
    for f in made["geom"]:
        if isinstance(f, bmesh.types.BMFace):
            f.material_index = rubber
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)


def inside(obj, points):
    """How many of `points` lie outside the surface of `obj` (a check that the foot fits)."""
    from mathutils.bvhtree import BVHTree

    tree = BVHTree.FromPolygons([v.co for v in obj.data.vertices], [tuple(p.vertices) for p in obj.data.polygons])
    out = 0
    for p in points:
        hit, normal, _, _ = tree.find_nearest(Vector(p))
        if hit is not None and (Vector(p) - hit).dot(normal) > 0:
            out += 1
    return out


def feet(skin, tag):
    """The skin's foot and ankle up to the shaft's top, the points the boot must hold."""
    co, _ = R.arrays(skin.data)
    w = R.weights(skin.data, (f"foot.{tag}", f"shank.{tag}"))
    return co[(w > 0.5) & (co[:, 2] < SHAFT[1] + 0.01)]
