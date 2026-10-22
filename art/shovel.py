"""The shovel (G1.4c; `tools.py`): a round point shovel with a long handle, as Asaka's Long-R
(1485 mm long, its blade 220 × 300 mm, 2.2 kg). A round handle held near its top end, a steel
socket over its other end, and the blade: a dished plate 2 mm thick, its sides straight to a
round point, set at a lift so it lies flatter than the handle. Origin at the grip, 10 cm below
the handle's top; the blade's hollow faces −y.
"""

import math

import bmesh
from common import finish, join, part
from mathutils import Matrix, Vector
from tools import bar, report, straight

# The handle (m along z, from the grip): a straight round shaft, its end in the socket.
HANDLE = [(-0.10, 0.0185, 0.0185), (-0.08, 0.019, 0.019), (0.5, 0.019, 0.019), (1.08, 0.018, 0.018)]
# The socket over the handle's end, from its mouth to where it meets the blade.
SOCKET = [(0.92, 0.0205, 0.0205), (1.06, 0.020, 0.020), (1.09, 0.024, 0.016)]
# The blade: its top at z = TOP, `LENGTH` to its point; half its width; straight sides for
# `SIDES` before they round to the point; its hollow `DISH` deep; its thickness; its lift.
TOP, LENGTH, HALF, SIDES = 1.09, 0.30, 0.11, 0.17
DISH, THICK, LIFT = 0.022, 0.002, math.radians(18.0)
ACROSS, ALONG = 16, 12


def handle():
    bm = bmesh.new()
    z0, z1 = HANDLE[0][0], HANDLE[-1][0]
    sections = [((z - z0) / (z1 - z0), a, b) for z, a, b in HANDLE]
    bar(bm, straight((0, 0, z0), (0, 0, z1)), sections, grain=2, end=0.006)
    return part("handle", bm, "timber")


def socket():
    bm = bmesh.new()
    z0, z1 = SOCKET[0][0], SOCKET[-1][0]
    sections = [((z - z0) / (z1 - z0), a, b) for z, a, b in SOCKET]
    bar(bm, straight((0, 0, z0), (0, 0, z1)), sections, grain=2, end=0.003)
    return part("socket", bm, "steel")


def half_width(s):
    """The blade's half width `s` down it from its top."""
    if s <= SIDES:
        return HALF
    x = min((s - SIDES) / (LENGTH - SIDES), 1.0)
    return HALF * math.sqrt(max(1.0 - x * x, 0.0))


def blade():
    """A dished plate, front and back and the rim between, tilted forward by its lift about its
    top."""
    bm = bmesh.new()
    part_layer = bm.faces.layers.int.get("part") or bm.faces.layers.int.new("part")
    grain_layer = bm.faces.layers.int.get("grain") or bm.faces.layers.int.new("grain")
    turn = Matrix.Rotation(LIFT, 3, "X")

    def point(i, j, back):
        # Closer together towards the point, where the outline turns fastest.
        s = LENGTH * (1.0 - (1.0 - j / ALONG) ** 1.7)
        w = max(half_width(s), 0.002)
        x = w * (2.0 * i / ACROSS - 1.0)
        y = -DISH * (x / HALF) ** 2 + (THICK if back else 0.0)
        return Vector((0.0, 0.0, TOP)) + turn @ Vector((x, y, s))

    grids = [[[bm.verts.new(point(i, j, back)) for i in range(ACROSS + 1)] for j in range(ALONG + 1)] for back in (False, True)]
    faces = []
    for k, g in enumerate(grids):
        for j in range(ALONG):
            for i in range(ACROSS):
                quad = (g[j][i], g[j][i + 1], g[j + 1][i + 1], g[j + 1][i])
                faces.append(bm.faces.new(quad if k else tuple(reversed(quad))))
    front, back = grids
    rim = [(j, 0) for j in range(ALONG + 1)] + [(ALONG, i) for i in range(1, ACROSS + 1)]
    rim += [(j, ACROSS) for j in range(ALONG - 1, -1, -1)] + [(0, i) for i in range(ACROSS - 1, 0, -1)]
    for (j0, i0), (j1, i1) in zip(rim, rim[1:] + rim[:1]):
        faces.append(bm.faces.new((front[j0][i0], front[j1][i1], back[j1][i1], back[j0][i0])))
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    from common import _last_part

    _last_part[0] += 1
    for f in bm.faces:
        f[part_layer] = _last_part[0]
        f[grain_layer] = 2
    return part("blade", bm, "steel")


def build():
    parts = [handle(), socket(), blade()]
    report("shovel", parts)
    return finish(join("shovel", parts))
