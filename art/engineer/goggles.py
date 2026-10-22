"""The engineer's goggles (G1.3f; `docs/art-style.md` §8), built with the outfit (`outfit.py`):
pushed up on the forehead, their leather cups pressing the hair flat, their strap a flat band
round the back of the head pressed into it. `gear.py` adds their rims and bridge."""

import math

import garments as G
from mathutils import Vector

# The strap: its height round the head, half height and thickness (m), and where it starts round
# from the front (degrees, beside the cups).
STRAP = (1.748, 0.012, 0.003, 40)


def goggles(p, skin):
    """Goggles pushed up on the forehead, their strap round the head: the face stays open. The
    hair is pressed flat under their cups (over their rims' tops, it came 3.9 mm through)."""
    hair = next(o for o in p.objects["body"] if o.name == "hair")
    facing = Vector((0, -1, 0.55)).normalized()
    for x in (-0.034, 0.034):
        near = [v.co for o in (skin, hair) for v in o.data.vertices if abs(v.co.x - x) < 0.006 and abs(v.co.z - 1.752) < 0.006]
        front = min(c.y for c in near)
        p.disc("goggles", "frame", Vector((x, front - 0.004, 1.752)), 0.028, 0.016, facing, "leather")
        p.disc("goggles", "lens", Vector((x, front - 0.011, 1.756)), 0.022, 0.012, facing, "lens")
        pressed(hair, skin, Vector((x, front - 0.004, 1.752)), facing, 0.0285, 0.008)
    # The strap a flat band round the back from cup to cup, pressed into the hair (a round tube
    # off it read as a hat's brim).
    z, half, thick, start = STRAP
    ring, middle = G.ring(hair, z)
    back = sorted((q for q in ring if abs(math.degrees(math.atan2(q.x - middle.x, middle.y - q.y))) > start), key=lambda q: math.atan2(q.x - middle.x, q.y - middle.y))
    banded(hair, skin, middle, back, z, half)
    path = [middle + (q - middle).normalized() * ((q - middle).length + thick / 2 - 0.001) for q in back]
    p.curve("goggles", "strap", path, "leather", flat=(half, thick), closed=False)


def banded(hair, skin, middle, band, z, half, depth=0.0015, ease=0.006):
    """`hair` pressed `depth` (m) under a band `half` high round `band` (its outermost points at
    the height `z`, in order round `middle`), easing out over `ease` above and below it; never
    closer than 1 mm to the `skin`."""
    from mathutils.bvhtree import BVHTree

    tree = BVHTree.FromPolygons([v.co.copy() for v in skin.data.vertices], [tuple(f.vertices) for f in skin.data.polygons])
    angles = [math.atan2(q.x - middle.x, q.y - middle.y) for q in band]
    reach = [(q - middle).xy.length for q in band]
    for v in hair.data.vertices:
        a = math.atan2(v.co.x - middle.x, v.co.y - middle.y)
        if not angles[0] - 0.05 < a < angles[-1] + 0.05 or abs(v.co.z - z) > half + ease:
            continue
        k = min(max(next((i for i, b in enumerate(angles) if b > a), len(angles) - 1), 1), len(angles) - 1)
        t = min(max((a - angles[k - 1]) / max(angles[k] - angles[k - 1], 1e-6), 0.0), 1.0)
        limit = reach[k - 1] + (reach[k] - reach[k - 1]) * t - depth
        d = v.co.xy - middle.xy
        if d.length <= limit:
            continue
        hold = 1.0 - G.smoothstep(half, half + ease, abs(v.co.z - z))
        r = d.length - (d.length - limit) * hold
        moved = Vector((middle.x + d.x * r / d.length, middle.y + d.y * r / d.length, v.co.z))
        q, n, _, _ = tree.find_nearest(moved)
        v.co = q + n * 0.001 if q is not None and (moved - q).dot(n) < 0.001 else moved
    hair.data.update()


def pressed(hair, skin, centre, facing, radius, depth, ease=0.006):
    """`hair` pressed behind a cup at `centre` (`radius` and `depth` m, its back `depth` behind
    its middle along `facing`), easing out over `ease` (m) round it; never closer than 1 mm to
    the `skin`."""
    from mathutils.bvhtree import BVHTree

    tree = BVHTree.FromPolygons([v.co.copy() for v in skin.data.vertices], [tuple(f.vertices) for f in skin.data.polygons])
    for v in hair.data.vertices:
        d = v.co - centre
        along = d.dot(facing)
        across = (d - facing * along).length
        if along <= -depth - 0.0015 or across > radius + ease:
            continue
        moved = v.co - facing * ((along + depth + 0.0015) * (1.0 - G.smoothstep(radius, radius + ease, across)))
        q, n, _, _ = tree.find_nearest(moved)
        if q is not None and (moved - q).dot(n) < 0.001:
            moved = q + n * 0.001
        v.co = moved
    hair.data.update()
