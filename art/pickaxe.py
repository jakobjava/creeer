"""The pickaxe (G1.4c; `tools.py`): a pick mattock as sold in 5 lb on a 36 in hickory handle
(Collins, Channellock, Ames: a 2.27 kg head 19–20 in long). The handle oval, swelling at its end
for the hand and at its top for the eye; the head forged in one: a pick tapering to a point
forward (−y) and an adze flattening to a blade behind, both curving down from the eye. Origin
at the grip, 7 cm above the handle's end.
"""

import bmesh
from common import finish, join, part
from mathutils import Vector
from tools import ROUND, bar, loft, report, ring, straight

HANDLE = [(-0.07, 0.018, 0.026), (-0.04, 0.016, 0.023), (0.05, 0.015, 0.021), (0.55, 0.015, 0.021),
          (0.74, 0.016, 0.024), (0.80, 0.019, 0.028), (0.845, 0.019, 0.028)]
# The head along y from the adze's blade (+y, behind) to the pick's point (−y): its half width
# (x) and half height (across its curve) there.
EYE_Z = 0.81
HEAD = [(0.235, 0.034, 0.0025), (0.20, 0.030, 0.004), (0.12, 0.020, 0.008), (0.05, 0.016, 0.014), (0.04, 0.018, 0.021),
        (0.03, 0.021, 0.030), (-0.03, 0.021, 0.030), (-0.04, 0.017, 0.022), (-0.05, 0.013, 0.015), (-0.13, 0.010, 0.011),
        (-0.21, 0.005, 0.006), (-0.25, 0.0012, 0.0015)]
# How far each arm's end curves down below the eye (m).
DROP = 0.035


def handle():
    bm = bmesh.new()
    z0, z1 = HANDLE[0][0], HANDLE[-1][0]
    sections = [((z - z0) / (z1 - z0), a, b) for z, a, b in HANDLE]
    bar(bm, straight((0, 0, z0), (0, 0, z1)), sections, grain=2, end=0.006)
    return part("handle", bm, "timber")


def centre(y):
    """The head's middle line: level at the eye, curving down towards either end."""
    reach = HEAD[0][0] if y > 0 else -HEAD[-1][0]
    return EYE_Z - DROP * (y / reach) ** 2


def head():
    bm = bmesh.new()
    rings = []
    for y, a, b in HEAD:
        h = 1e-4
        slope = (centre(y + h) - centre(y - h)) / (2 * h)
        tangent = Vector((0.0, 1.0, slope)).normalized()
        across = Vector((1.0, 0.0, 0.0))
        up = tangent.cross(across).normalized()
        if up.z < 0:
            up = -up
        # The adze's blade wider than thick, the pick's bar about square, the eye between.
        square = 0.8 if y > 0.1 else 0.4
        rings.append(ring(Vector((0.0, y, centre(y))), across, up, a, b, n=ROUND, square=square))
    loft(bm, rings, grain=1)
    return part("head", bm, "steel")


def build():
    parts = [handle(), head()]
    report("pickaxe", parts)
    return finish(join("pickaxe", parts))

