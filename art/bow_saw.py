"""The bow saw (G1.4c; `tools.py`): a 24 in bow saw as Bahco's 10-24-51 (a 607 mm blade,
743 g). A bow of painted steel tube, oval, from its grip upright over in a long curve to the
nose; the blade tensioned along the bottom, its teeth down; a timber sleeve on the upright for
the hand. Origin at the grip, the blade running forward (−y) from below it.
"""

import bmesh
from common import finish, join, part
from mathutils import Vector
from tools import ROUND, bar, loft, report, ring, straight

# The blade: from the upright forward, its top and its height, thickness, the teeth's pitch and
# depth (m).
BACK, BLADE, TOP, HEIGHT, THICK = 0.0, 0.607, -0.080, 0.025, 0.0009
PITCH, TOOTH = 0.010, 0.004
# The bow's centre line (y, z) from the blade's back up the upright, over and down to the nose,
# and its tube's half widths across (x) and in its plane (m), its wall.
BOW = [(0.0, -0.08), (0.0, 0.0), (0.005, 0.11), (-0.04, 0.16), (-0.15, 0.17), (-0.32, 0.12),
       (-0.48, 0.03), (-0.585, -0.06), (-0.607, -0.08)]
TUBE, WALL = (0.007, 0.011), 0.0012
# The grip's sleeve on the upright (m along z), oval.
GRIP = [(-0.055, 0.015, 0.019), (0.0, 0.016, 0.020), (0.07, 0.015, 0.019)]
STEPS = (1, 2, 5, 6, 3, 3, 3, 2)


def catmull(points, t):
    """A point of the Catmull–Rom curve through `points` at `t` in [0, 1]."""
    n = len(points) - 1
    k = min(int(t * n), n - 1)
    u = t * n - k
    p0, p1 = points[max(k - 1, 0)], points[k]
    p2, p3 = points[k + 1], points[min(k + 2, n)]
    return 0.5 * ((2 * p1) + (-p0 + p2) * u + (2 * p0 - 5 * p1 + 4 * p2 - p3) * u * u + (-p0 + 3 * p1 - 3 * p2 + p3) * u**3)


def bow():
    points = [Vector((0.0, y, z)) for y, z in BOW]
    segments = len(points) - 1
    # Each segment of the curve in as many rings as it turns: few up the straight upright, most
    # round the bend over it.
    ts = [0.0]
    for k in range(segments):
        steps = STEPS[k]
        ts += [(k + j / steps) / segments for j in range(1, steps + 1)]
    rings = []
    across = Vector((1.0, 0.0, 0.0))
    for t in ts:
        c = catmull(points, t)
        ahead = catmull(points, min(t + 1e-3, 1.0)) - catmull(points, max(t - 1e-3, 0.0))
        tangent = ahead.normalized()
        v = tangent.cross(across).normalized()
        rings.append(ring(c, across, v, TUBE[0], TUBE[1], n=ROUND))
    bm = bmesh.new()
    loft(bm, rings, grain=1)
    obj = part("bow", bm, "steel_painted")
    obj["wall"] = WALL
    return obj


def blade():
    """The blade's outline, its teeth along the bottom, as a plate `THICK` thick."""
    bm = bmesh.new()
    teeth = int(BLADE / PITCH)
    outline = [(BACK, TOP), (BACK - BLADE, TOP)]
    for k in range(teeth):
        y = BACK - BLADE + k * PITCH
        outline += [(y, TOP - HEIGHT), (y + 0.5 * PITCH, TOP - HEIGHT - TOOTH)]
    outline.append((BACK, TOP - HEIGHT))
    front = [bm.verts.new((-0.5 * THICK, y, z)) for y, z in outline]
    back = [bm.verts.new((0.5 * THICK, y, z)) for y, z in outline]
    faces = [bm.faces.new(front), bm.faces.new(list(reversed(back)))]
    n = len(outline)
    for k in range(n):
        faces.append(bm.faces.new((front[(k + 1) % n], front[k], back[k], back[(k + 1) % n])))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    from common import _last_part

    part_layer = bm.faces.layers.int.get("part") or bm.faces.layers.int.new("part")
    grain_layer = bm.faces.layers.int.get("grain") or bm.faces.layers.int.new("grain")
    _last_part[0] += 1
    for f in bm.faces:
        f[part_layer] = _last_part[0]
        f[grain_layer] = 1
    return part("blade", bm, "steel")


def grip():
    bm = bmesh.new()
    z0, z1 = GRIP[0][0], GRIP[-1][0]
    sections = [((z - z0) / (z1 - z0), a, b) for z, a, b in GRIP]
    bar(bm, straight((0, 0, z0), (0, 0, z1)), sections, grain=2, end=0.005)
    return part("grip", bm, "timber")


def build():
    parts = [bow(), blade(), grip()]
    report("bow_saw", parts)
    return finish(join("bow_saw", parts))
