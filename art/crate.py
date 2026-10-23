"""The sandbox's crate (G1.3b, the first model through the pipeline).

Its physics is a 1 m box (creeer_server world/commands.rs), so the model fills exactly that box,
its origin at the centre of its base (docs/art-style.md §7). Timber boards, four to a side, on a
frame of square battens along the twelve edges, with painted steel plates on the eight corners.
"""

import bmesh

from common import bevel_edges, box, finish, join, part

SIZE = 1.0
HALF = SIZE / 2
# Corner plates: thickness and reach along each edge (m).
PLATE = 0.006
REACH = 0.13
# The battens' outer faces sit under the plates; their square section (m).
OUT = HALF - PLATE
BATTEN = 0.07
# Boards: recess behind the battens' faces, thickness, gap between boards (m).
RECESS = 0.012
BOARD = 0.022
GAP = 0.008
BOARDS = 4
# Chamfers on the battens' and boards' long edges (m): art-style.md §7, at least 5 mm.
CHAMFER = 0.006


def _span(sign, depth):
    """The range of one axis for a part `depth` thick against the face on side `sign`."""
    return (OUT - depth, OUT) if sign > 0 else (-OUT, -OUT + depth)


def _long_edges(faces, axis):
    """The edges of `faces` that run along `axis` (0, 1, 2)."""
    edges = dict.fromkeys(e for f in faces for e in f.edges)  # ordered, unlike a set
    out = []
    for e in edges:
        d = e.verts[1].co - e.verts[0].co
        if abs(d[axis]) > 0.9 * d.length:
            out.append(e)
    return out


def battens():
    """Twelve square battens along the edges, chamfered along their length."""
    bm = bmesh.new()
    z0, z1 = PLATE, SIZE - PLATE
    inner = OUT - BATTEN
    bars = []
    for sx in (-1, 1):
        for sy in (-1, 1):
            # Vertical, at the corners.
            x, y = _span(sx, BATTEN), _span(sy, BATTEN)
            bars.append(((x[0], y[0], z0), (x[1], y[1], z1), 2))
    for sz in (-1, 1):
        z = (z0, z0 + BATTEN) if sz < 0 else (z1 - BATTEN, z1)
        for s in (-1, 1):
            # Along x, at y = ±, and along y, at x = ±: between the corner battens.
            y = _span(s, BATTEN)
            bars.append(((-inner, y[0], z[0]), (inner, y[1], z[1]), 0))
            x = _span(s, BATTEN)
            bars.append(((x[0], -inner, z[0]), (x[1], inner, z[1]), 1))
    for lo, hi, axis in bars:
        faces = box(bm, lo, hi)
        bevel_edges(bm, _long_edges(faces, axis), CHAMFER)
    return part("battens", bm, "timber")


def _board(bm, lo, hi, length_axis, out_axis, out_sign):
    """One board: its hidden inner face and ends removed, its two outer long edges chamfered."""
    faces = box(bm, lo, hi)
    hidden = []
    for f in faces:
        n = f.normal
        if abs(n[length_axis]) > 0.9 or n[out_axis] * out_sign < -0.9:
            hidden.append(f)
    keep = [f for f in faces if f not in hidden]
    bmesh.ops.delete(bm, geom=hidden, context="FACES_ONLY")
    outer = [f for f in keep if f.normal[out_axis] * out_sign > 0.9]
    long = [e for e in _long_edges(outer, length_axis) if len(e.link_faces) == 2]
    bevel_edges(bm, long, CHAMFER)


def boards():
    """Four boards on each side and on the top, recessed behind the battens."""
    bm = bmesh.new()
    face_out, face_in = OUT - RECESS, OUT - RECESS - BOARD
    # Boards run under the battens at their ends, hidden there.
    run = OUT - BATTEN + 0.01
    span = SIZE - 2 * PLATE - 2 * BATTEN
    height = (span - (BOARDS - 1) * GAP) / BOARDS
    for k in range(BOARDS):
        a = PLATE + BATTEN + k * (height + GAP)
        b = a + height
        for s in (-1, 1):
            depth = (face_in, face_out) if s > 0 else (-face_out, -face_in)
            # Sides facing ±x run along y; sides facing ±y run along x.
            _board(bm, (depth[0], -run, a), (depth[1], run, b), 1, 0, s)
            _board(bm, (-run, depth[0], a), (run, depth[1], b), 0, 1, s)
        # The top, along x, its boards side by side in y.
        y0 = -HALF + PLATE + BATTEN + k * (height + GAP)
        top = (SIZE - PLATE - RECESS - BOARD, SIZE - PLATE - RECESS)
        _board(bm, (-run, y0, top[0]), (run, y0 + height, top[1]), 0, 2, 1)
    return part("boards", bm, "timber")


def core():
    """The closed box behind the boards: seen in the gaps, and as the crate's bottom."""
    bm = bmesh.new()
    inner = OUT - RECESS - BOARD
    # Its bottom a few millimetres up from the battens' bottoms, so the two never share a plane.
    box(bm, (-inner, -inner, PLATE + 0.004), (inner, inner, SIZE - PLATE - RECESS - BOARD))
    return part("core", bm, "timber")


def plates():
    """Painted steel plates over the eight corners: one on each of the three faces there."""
    bm = bmesh.new()
    for sx in (-1, 1):
        for sy in (-1, 1):
            for sz in (-1, 1):
                def r(sign, lo, hi):
                    return (lo, hi) if sign > 0 else (-hi, -lo)

                zr = (SIZE - REACH, SIZE - PLATE) if sz > 0 else (PLATE, REACH)
                zcap = (SIZE - PLATE, SIZE) if sz > 0 else (0.0, PLATE)
                pieces = [
                    # On the top or bottom: the whole corner square.
                    (r(sx, HALF - REACH, HALF), r(sy, HALF - REACH, HALF), zcap),
                    # On the ±y face, under the cap.
                    (r(sx, HALF - REACH, HALF), r(sy, OUT, HALF), zr),
                    # On the ±x face, between the cap and the ±y plate.
                    (r(sx, OUT, HALF), r(sy, HALF - REACH, OUT), zr),
                ]
                for (x0, x1), (y0, y1), (z0, z1) in pieces:
                    faces = box(bm, (x0, y0, z0), (x1, y1, z1))
                    # The face against the crate is hidden.
                    centre_dir = [-sx * (abs(x1 - x0) < 0.01), -sy * (abs(y1 - y0) < 0.01),
                                  -sz * (abs(z1 - z0) < 0.01)]
                    hidden = [f for f in faces
                              if sum(f.normal[i] * centre_dir[i] for i in range(3)) > 0.9]
                    bmesh.ops.delete(bm, geom=hidden, context="FACES_ONLY")
    return part("plates", bm, "steel_painted")


def build():
    """The crate, joined, shaded, with its occlusion and wear baked."""
    obj = join("crate", [battens(), boards(), core(), plates()])
    return finish(obj)
