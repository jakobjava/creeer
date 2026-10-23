"""The boots' detail (G1.3f; `detail.py`): work boots as they are made (the boot-anatomy survey,
ADR-0026: toe cap, counter, eyestays with punched eyelets and speed hooks, a padded collar,
the upper on a welt over a lugged rubber sole). On each boot of `outfit.py`'s pair, in the
ragdoll's frame:

- a toe cap and a heel counter sewn over the upper (raised 1.8 mm);
- the eyestays either side of the tongue, five eyelet pairs and two hook pairs in steel, the
  laces crossing between them and tied at the top;
- a padded collar rolled round the top, a pull tab at the back.

The boot itself and its sole are built on a last (`last.py`) in place of the blockout's.
"""

import math

import bmesh
import numpy as np
from mathutils import Vector

import last as L
import relief as R
import trims as T

SIDES = ((1, "L"), (-1, "R"))
UP = Vector((0, 0, 1))
# The lacing (m): from the instep (ahead of the ankle, up) to the top, the eyestays' reach either
# side of the middle, the eyelets' and hooks' places along it (0 the instep, 1 the top).
LACING = ((0.08, 0.07), (0.03, 0.215))
STAY = 0.017
EYELETS = (0.06, 0.18, 0.30, 0.42, 0.54)
HOOKS = (0.72, 0.88)


def along(obj, side, ankle, ahead):
    """The boot's vertices on `side`, and how far ahead of the ankle each lies."""
    co, _ = R.arrays(obj.data)
    on = side * co[:, 0] > 0
    return on, (co - np.array(ankle)) @ np.array(ahead)


def caps(obj):
    """A toe cap and a heel counter on each boot."""
    for side, tag in SIDES:
        ankle, ahead = L.frame(tag)
        on, d = along(obj, side, ankle, ahead)
        tip, heel = float(d[on].max()), float(d[on].min())
        mine = lambda bm, side=side: (lambda v: side * v.co.x > 0)  # noqa: E731
        R.panel(obj, [(ankle + ahead * (tip - 0.065), -ahead), ((0, 0, 0.11), (0, 0, 1))], 0.0018, mine)
        R.panel(obj, [(ankle + ahead * (heel + 0.075), ahead), ((0, 0, 0.09), (0, 0, 1))], 0.0018, mine)


def lacing_line(surface, tag):
    """The lacing's middle line down the boot's front: points on the surface, their normals, and
    the direction across it (towards +x), at `count` steps from the instep to the top."""
    ankle, ahead = L.frame(tag)
    (a_ahead, a_up), (b_ahead, b_up) = LACING
    start = Vector((ankle.x, ankle.y, 0)) + ahead * a_ahead + UP * a_up
    end = Vector((ankle.x, ankle.y, 0)) + ahead * b_ahead + UP * b_up
    points, normals = [], []
    for k in range(25):
        s = k / 24
        p = start.lerp(end, s)
        out = (UP * 0.7 + ahead * 0.7).lerp(ahead, s ** 0.7).normalized()
        hit, normal = surface.cast(p + out * 0.12, -out)
        points.append(hit)
        normals.append(normal)
    across = []
    for k in range(len(points)):
        tangent = points[min(k + 1, len(points) - 1)] - points[max(k - 1, 0)]
        a = tangent.cross(normals[k]).normalized()
        across.append(a if a.x > 0 else -a)
    return points, normals, across


def sample(line, s):
    """The lacing line's point, normal and across at `s` (0–1)."""
    points, normals, across = line
    x = s * (len(points) - 1)
    k = min(int(x), len(points) - 2)
    f = x - k
    return points[k].lerp(points[k + 1], f), normals[k].lerp(normals[k + 1], f).normalized(), across[k].lerp(across[k + 1], f).normalized()


def eyestays(obj, line):
    """The eyestays raised either side of the lacing, the tongue sunk between them."""
    points, normals, across = (np.array([tuple(v) for v in x]) for x in line)
    co, _ = R.arrays(obj.data)
    best = np.full(len(co), np.inf)
    lateral, depth, s = np.zeros(len(co)), np.zeros(len(co)), np.zeros(len(co))
    for k, (p, n, a) in enumerate(zip(points, normals, across)):
        d = co - p
        dist = np.linalg.norm(d - np.outer(d @ n, n), axis=1)
        closer = dist < best
        best = np.where(closer, dist, best)
        lateral = np.where(closer, d @ a, lateral)
        depth = np.where(closer, d @ n, depth)
        s = np.where(closer, k / (len(points) - 1), s)
    near = (best < 0.04) & (np.abs(depth) < 0.012)
    l = np.abs(lateral)
    stay = R.smoothstep(0.006, 0.009, l) * (1.0 - R.smoothstep(STAY + 0.009, STAY + 0.013, l))
    tongue = 1.0 - R.smoothstep(0.004, 0.008, l)
    ends = R.smoothstep(0.0, 0.04, s) * (1.0 - R.smoothstep(0.97, 1.0, s))
    R.displace(obj.data, near * ends * (0.0015 * stay - 0.0012 * tongue))


def laces(metal, bm, surface, line):
    """Eyelets and hooks either side (into `metal`), the laces crossing between them, a knot
    and bow on top (into `bm`)."""
    holes = []
    for s in EYELETS + HOOKS:
        p, n, a = sample(line, s)
        pair = []
        for sign in (1, -1):
            hit, normal = surface.nearest(p + a * (sign * STAY))
            if s in HOOKS:
                T.ring(metal, hit + normal * 0.0035, a * sign, 0.0033, 0.0011, segments=8, sides=4, up=normal)
            else:
                T.ring(metal, hit + normal * 0.0006, normal, 0.003, 0.001, segments=8, sides=4)
            pair.append((hit + normal * 0.0012, normal))
        holes.append(pair)
    for (l0, r0), (l1, r1) in zip(holes, holes[1:]):
        for (a, na), (b, nb) in ((l0, r1), (r0, l1)):
            lift = (na + nb).normalized() * 0.0035
            T.tube(bm, [a, (a + b) / 2 + lift, b], 0.0013, sides=4)
    (lt, nt), (rt, _) = holes[-1]
    knot = (lt + rt) / 2 + nt * 0.005
    across = (lt - rt).normalized()
    down = -sample(line, 1.0)[0] + sample(line, 0.8)[0]
    down.normalize()
    for sign in (1, -1):
        T.tube(bm, [lt if sign > 0 else rt, knot], 0.0013, sides=4)
        loop = [knot + across * (sign * (0.011 + 0.01 * math.cos(a))) + (UP * 0.4 + nt).normalized() * (0.007 * math.sin(a)) for a in np.linspace(math.pi, -math.pi, 9)[:-1]]
        T.tube(bm, loop, 0.0013, sides=4, closed=True)
        T.tube(bm, [knot, knot + down * 0.02 + across * (sign * 0.004) + nt * 0.003, knot + down * 0.04 + across * (sign * 0.007) + nt * 0.002], 0.0013, sides=4)


def collar(bm, obj, side):
    """A padded collar rolled round the boot's top, and a pull tab at the back."""
    mine = [v.co for v in obj.data.vertices if side * v.co.x > 0]
    top = max(c.z for c in mine)
    rim = [c.copy() for c in mine if c.z > top - 0.004]
    middle = sum(rim, Vector()) / len(rim)
    bins = {}
    for c in rim:
        k = round(math.degrees(math.atan2(c.x - middle.x, c.y - middle.y)) / 15)
        bins.setdefault(k, []).append(c)
    loop = [sum(cs, Vector()) / len(cs) for _, cs in sorted(bins.items())]
    thick, out, _ = L.COLLAR
    loop = [c + (Vector((c.x - middle.x, c.y - middle.y, 0)).normalized() * out) for c in loop]
    T.tube(bm, loop, thick, sides=6, closed=True)
    back = max(loop, key=lambda c: c.y)
    out = Vector((back.x - middle.x, back.y - middle.y, 0)).normalized()
    # A loop in a smooth arc (bent at right angles, its tube's faces strayed from the curve).
    side = UP.cross(out).normalized()
    tab = [back + out * 0.004 - UP * 0.004 + side * (0.008 * math.cos(a)) + UP * (0.018 * math.sin(a)) for a in np.linspace(math.pi, 0, 9)]
    T.tube(bm, tab, 0.003, sides=6)


def detail(p):
    """The boots built anew on their lasts, then detailed: the toe caps and counters, the
    lacing, the collars and tabs, the soles."""
    import bpy

    import outfit as O

    skin = next(o for o in p.objects["body"] if o.name == "skin")
    for old in list(p.objects["boots"]):
        p.objects["boots"].remove(old)
        bpy.data.objects.remove(old)
    uppers = [L.upper(p.scene, skin, tag, side) for side, tag in SIDES]
    outlines = {tag: L.outline_of(obj) for (_, tag), obj in zip(SIDES, uppers)}
    for (side, tag), obj in zip(SIDES, uppers):
        held = [c for c in L.feet(skin, tag) if L.SOLE_TOP + 0.004 < c[2] < L.SHAFT[1] - 0.01]
        print(f"art: boot.{tag}: {L.inside(obj, held)} of {len(held)} points of the foot outside it")
    with bpy.context.temp_override(selected_editable_objects=uppers, active_object=uppers[0], object=uppers[0]):
        bpy.ops.object.join()
    boots = uppers[0]
    boots.name = boots.data.name = "boots"
    boots["host"] = skin.name
    boots["only"] = [f"{s}.{t}" for _, t in SIDES for s in ("shank", "foot")]
    p.add("boots", boots, "leather")
    caps(boots)
    for side, tag in SIDES:
        eyestays(boots, lacing_line(T.Surface(boots), tag))
    surface = T.Surface(boots)
    for side, tag in SIDES:
        line = lacing_line(surface, tag)
        only = (f"shank.{tag}", f"foot.{tag}")
        metal, bm = bmesh.new(), bmesh.new()
        laces(metal, bm, surface, line)
        T.place(p, "boots", f"eyelets.{tag}", metal, "steel", boots, only)["hardware"] = "close"
        T.place(p, "boots", f"laces.{tag}", bm, "leather", boots, only)["hardware"] = "close"
        bm = bmesh.new()
        collar(bm, boots, side)
        T.place(p, "boots", f"collar.{tag}", bm, "leather", boots, only)["hardware"] = "mid"
        bm = bmesh.new()
        L.sole(bm, outlines[tag], tag, side, 0, 1)
        obj = T.place(p, "boots", f"sole.{tag}", bm, "rubber", boots)
        obj.data.materials.append(O.token("leather"))
        obj["rigid"] = f"foot.{tag}"
        # Whole close up: decimated with the boot, its sides collapsed into fans.
        obj["hardware"] = "whole"
        T.sharpen(obj.data, 40)
        # The tread shaded flat: one fill across the lifted arch, its long triangles shaded
        # smooth strayed a centimetre from any curve.
        for f in obj.data.polygons:
            if f.normal.z < -0.7:
                f.use_smooth = False
