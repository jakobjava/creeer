"""The axe (G1.4c; `tools.py`): a felling axe as Gränsfors Bruk's American felling axe is made,
81 cm long, 2.1 kg (Highland Woodworking's listing; its head's mass taken as 1.6 kg here, the
listings disagreeing). An oval hickory handle with a swell at its end for the hand; a forged
head: its poll behind the eye flat, its top nearly straight, its cheeks thinning to a bit that
flares down to a convex edge 11 cm long. Origin at the grip, 6 cm above the handle's end; the
edge faces −y.
"""

import bmesh
from common import finish, join, part
from mathutils import Vector
from tools import ROUND, bar, loft, report, ring, straight

# The handle from its end to its top (m along z, from the grip), and its oval's half widths
# across the swing (x) and along it (y) there; its top flush with the head's.
HANDLE = [(-0.06, 0.016, 0.022), (-0.03, 0.014, 0.020), (0.05, 0.0125, 0.018), (0.45, 0.012, 0.017),
          (0.62, 0.0115, 0.016), (0.69, 0.013, 0.020), (0.735, 0.013, 0.020)]
# The head along −y from the poll to the edge (m): its half thickness (x), its top and its
# bottom (z) there.
HEAD = [(0.036, 0.012, 0.731, 0.669), (0.034, 0.016, 0.735, 0.665), (0.02, 0.018, 0.736, 0.664),
        (-0.03, 0.017, 0.736, 0.664), (-0.06, 0.009, 0.737, 0.658), (-0.09, 0.005, 0.739, 0.645),
        (-0.105, 0.0038, 0.740, 0.639), (-0.115, 0.003, 0.741, 0.634), (-0.122, 0.0022, 0.7415, 0.632),
        (-0.128, 0.0002, 0.742, 0.631)]
# How far the edge bows out at its middle (m): a convex edge, as a felling bit is ground.
BOW = 0.012


def handle():
    bm = bmesh.new()
    z0, z1 = HANDLE[0][0], HANDLE[-1][0]
    sections = [((z - z0) / (z1 - z0), a, b) for z, a, b in HANDLE]
    bar(bm, straight((0, 0, z0), (0, 0, z1)), sections, grain=2, end=0.006)
    return part("handle", bm, "timber")


def head():
    """Ring by ring from the poll to the edge, each a rounded rectangle across the head; along
    the bit each ring's middle pushed out towards the edge's bow."""
    bm = bmesh.new()
    rings = []
    y_bit = HEAD[3][0]
    for y, a, top, bottom in HEAD:
        centre = Vector((0.0, y, 0.5 * (top + bottom)))
        half = 0.5 * (top - bottom)
        points = ring(centre, Vector((1.0, 0.0, 0.0)), Vector((0.0, 0.0, 1.0)), a, half, n=ROUND, square=0.85)
        share = max(0.0, (y_bit - y) / (y_bit - HEAD[-1][0]))
        for p in points:
            across = (p.z - centre.z) / half
            p.y -= BOW * share * share * max(0.0, 1.0 - across * across)
        rings.append(points)
    # The edge where the cheeks meet: its ring closed almost to a line, no cap over it.
    loft(bm, rings, grain=1, caps=(True, False))
    return part("head", bm, "steel")


def build():
    parts = [handle(), head()]
    report("axe", parts)
    return finish(join("axe", parts))
