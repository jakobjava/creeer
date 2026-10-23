"""The engineer's fingers (G1.4b; ADR-0027 decision 2): three bones a finger and the thumb, found
on the posed skin's hands and weighted from each vertex's hand weight at the export, so every
layer over the hand (the skin, the glove and its strap) bends its fingers alike.

- **The fingers** are the hand's skin past the webs, parted by the mesh's connectivity: beyond
  the webs each finger is its own piece, the thumb beyond the first web. Each finger's
  centreline runs through its sections' centres.
- **The joints.** The web lies about the middle of the proximal phalanx (the commissure ends
  near its mid-shaft); the knuckle (MCP) to the middle joint (PIP) is as long as the middle
  joint to the tip, and the middle joint to the last (DIP) 1.3 times the last to the tip, 1.0
  in the little finger (Hamilton and Dunsmuir 2002, joint centres by dissection, distances on
  radiographs). So with D the web to the tip, MCP–PIP = 2D/3 and the knuckle lies D/3 before
  the web. The thumb's from Buchholz, Armstrong and Goldstein (1992) as fractions of the hand's
  length: the metacarpal 0.251, the proximal phalanx 0.196, the distal 0.158 (as recalled, the
  paper's table not to hand; the metacarpal checks with a CT survey's 47.5 mm in men).
- **The weights.** Each vertex's hand weight is shared between the hand and the bones of the
  digit nearest it, by where it lies along that digit's centreline, blended across each joint
  (a knuckle's skin goes with both bones beside it).
"""

import collections
import math

from mathutils import Vector

DIGITS = ("thumb", "index", "middle", "ring", "little")
# A finger's middle joint to its last, over its last to the tip (Hamilton and Dunsmuir 2002).
DIP_RATIO = {"index": 1.3, "middle": 1.3, "ring": 1.3, "little": 1.0}
# The thumb's bones as fractions of the hand's length (Buchholz et al. 1992).
THUMB = (0.251, 0.196, 0.158)
# Each finger's knuckle to its tip, as fractions of the hand's length: its phalanges' sum
# (Buchholz et al. 1992: index 0.265 + 0.143 + 0.097, middle 0.277 + 0.170 + 0.108, ring
# 0.259 + 0.165 + 0.107, little 0.206 + 0.117 + 0.093).
FINGER = {"index": 0.505, "middle": 0.555, "ring": 0.531, "little": 0.416}
# How far either side of each joint its two bones share the skin (m): the knuckles', the
# fingers' other joints', the thumb's base.
BLEND = {"thumb": (0.015, 0.008, 0.006), "finger": (0.010, 0.006, 0.005)}
# Further from a digit's bones than this (m), the hand's own (the palm, the back of the hand).
NEAR = 0.025
# Pieces smaller than this are not fingers (the mesh's specks at a threshold).
SMALLEST = 50


def bone(digit, k, tag):
    """The name of digit `digit`'s bone `k` (1 the nearest the hand) on side `tag`."""
    return f"{digit}.{k}.{tag}"


def names():
    """Every finger bone's name, in the skin's order: by side (R, L, as the segments), digit,
    bone."""
    return [bone(d, k, tag) for tag in "RL" for d in DIGITS for k in (1, 2, 3)]


def _pieces(hand, t, adjacent, threshold):
    """The hand's vertices past `threshold` along it, in connected pieces (largest first)."""
    seen, out = set(), []
    for i in hand:
        if t[i] < threshold or i in seen:
            continue
        stack, piece = [i], []
        seen.add(i)
        while stack:
            x = stack.pop()
            piece.append(x)
            for y in adjacent[x]:
                if y not in seen and t[y] >= threshold:
                    seen.add(y)
                    stack.append(y)
        if len(piece) >= SMALLEST:
            out.append(piece)
    return sorted(out, key=len, reverse=True)


def _first(hand, t, adjacent, count):
    """The lowest threshold (1 mm steps) at which the hand parts into `count` pieces."""
    top = max(t[i] for i in hand)
    k = 0
    while k * 0.001 < top:
        pieces = _pieces(hand, t, adjacent, k * 0.001)
        if len(pieces) >= count:
            return pieces
        k += 1
    raise ValueError(f"the hand never parts into {count} pieces")


def _centreline(co, piece, wrist):
    """A digit's axis (towards its tip, away from the `wrist`), its base's centre and its tip's
    place along the axis: the axis through its sections' centres, from the base to near the tip."""
    points = [co[i] for i in piece]
    middle = sum(points, Vector()) / len(points)
    # Towards its point furthest from the wrist, as a first axis; then through the centres of
    # its sections.
    far = max(points, key=lambda p: (p - wrist).length)
    axis = (far - middle).normalized()
    base = min(points, key=lambda p: (p - middle).dot(axis))
    s = [(p - base).dot(axis) for p in points]
    length = max(s)
    centres = []
    for k in range(6):
        lo, hi = length * k / 7, length * (k + 1) / 7
        ring = [p for p, x in zip(points, s) if lo <= x < hi]
        if ring:
            centres.append(sum(ring, Vector()) / len(ring))
    axis = (centres[-1] - centres[0]).normalized()
    start = centres[0]
    s = [(p - start).dot(axis) for p in points]
    base_centre = start + axis * min(s)
    return axis, base_centre, max(s) - min(s)


def measure(mesh, weight, wrist):
    """The digits' joints on one hand of `mesh` (its vertices posed: the model at rest): each
    digit's four points, the joint nearest the hand first and its tip last. `weight` gives each
    vertex's weight on the hand's segment; `wrist` is the hand's joint."""
    hand = [v.index for v in mesh.vertices if weight[v.index] > 0.5]
    co = {i: mesh.vertices[i].co.copy() for i in hand}
    held = set(hand)
    adjacent = collections.defaultdict(list)
    for e in mesh.edges:
        a, b = e.vertices
        if a in held and b in held:
            adjacent[a].append(b)
            adjacent[b].append(a)
    furthest = max(co.values(), key=lambda p: (p - wrist).length)
    along = (furthest - wrist).normalized()
    t = {i: (co[i] - wrist).dot(along) for i in hand}
    two = _first(hand, t, adjacent, 2)
    thumb = two[1]
    fingers = _first(hand, t, adjacent, 4)[:4]
    # Across the palm from the thumb: the index nearest it.
    thumb_centre = sum((co[i] for i in thumb), Vector()) / len(thumb)
    fingers.sort(key=lambda p: (sum((co[i] for i in p), Vector()) / len(p) - thumb_centre).length)
    lines = {}
    for digit, piece in zip(DIGITS[1:], fingers):
        axis, web, d = _centreline(co, piece, wrist)
        lines[digit] = (axis, web + axis * d)
    length = (lines["middle"][1] - wrist).length
    out = {}
    for digit, (axis, tip) in lines.items():
        # The knuckle a finger's length (Buchholz et al.) back from its tip; the middle joint
        # halfway, the last as Hamilton and Dunsmuir found.
        whole = FINGER[digit] * length
        mcp = tip - axis * whole
        pip = mcp + axis * (whole / 2.0)
        r = DIP_RATIO[digit]
        dip = pip + axis * (whole / 2.0 * r / (1.0 + r))
        out[digit] = [mcp, pip, dip, tip]
    axis, web, d = _centreline(co, thumb, wrist)
    tip = web + axis * d
    ip = tip - axis * (THUMB[2] * length)
    mcp = ip - axis * (THUMB[1] * length)
    # The metacarpal from the thumb's knuckle back towards the wrist's radial side.
    radial = (thumb_centre - wrist) - along * (thumb_centre - wrist).dot(along)
    base = wrist + radial.normalized() * 0.02
    cmc = mcp + (base - mcp).normalized() * (THUMB[0] * length)
    out["thumb"] = [cmc, mcp, ip, tip]
    return out


def _smooth(x):
    x = min(max(x, 0.0), 1.0)
    return x * x * (3.0 - 2.0 * x)


def shares(digits, p):
    """How a vertex at `p` with its weight on the hand shares it: (bone index 0 the hand, 1–3
    the nearest digit's bones), the digit, and the shares."""
    best, digit, s = None, None, 0.0
    for name, points in digits.items():
        for a, b in zip(points, points[1:]):
            ab = b - a
            x = min(max((p - a).dot(ab) / ab.length_squared, 0.0), 1.0)
            d = (p - (a + ab * x)).length
            if best is None or d < best:
                best, digit = d, name
    points = digits[digit]
    # Along the digit: from its first joint, measured on its bones' line.
    along = 0.0
    a0 = points[0]
    first_axis = (points[1] - a0).normalized()
    along = (p - a0).dot(first_axis)
    joints_at = [0.0]
    total = 0.0
    for a, b in zip(points, points[1:]):
        total += (b - a).length
        joints_at.append(total)
    # Past the first bone, measured along the digit's later bones.
    if along > joints_at[1]:
        for k in range(1, 3):
            a, b = points[k], points[k + 1]
            x = (p - a).dot((b - a).normalized())
            if x >= 0.0:
                along = joints_at[k] + x
    blends = BLEND["thumb" if digit == "thumb" else "finger"]
    if best > NEAR:
        return digit, [1.0, 0.0, 0.0, 0.0]
    f = [_smooth((along - joints_at[k]) / (2.0 * blends[k]) + 0.5) for k in range(3)]
    return digit, [1.0 - f[0], f[0] - f[1], f[1] - f[2], f[2]]


# Bends for the fit check (degrees, from the hand out): relaxed as measured (Lee, Mo, Hwang,
# Wang and Jung; `animate/hands.rs`), and a fist about the game's grip on a 35 mm handle.
RELAXED = {
    "thumb": (0.0, 46.1, 8.5),
    "index": (28.4, 25.5, 13.1),
    "middle": (32.8, 30.1, 12.7),
    "ring": (24.7, 34.5, 11.7),
    "little": (16.6, 32.1, 15.6),
}
FIST = {"thumb": (0.0, 57.0, 69.0), **{d: (40.0, 80.0, 55.0) for d in DIGITS[1:]}}


def pose(rig, bends):
    """Poses `rig`'s finger bones bent `bends` (digit -> degrees from the hand out) on its
    hands as posed, each about its head towards the palm (the thumb towards the index), as the
    game bends them (`animate/hands.rs`; there the fingers also close together)."""
    from mathutils import Matrix

    bones = rig.data.bones
    for tag, outward in (("L", 1.0), ("R", -1.0)):
        heads = {d: [bones[bone(d, k, tag)].head_local.copy() for k in (1, 2, 3)] for d in DIGITS}
        hand = rig.pose.bones[f"hand.{tag}"]
        wrist = bones[f"hand.{tag}"].head_local
        across = (heads["index"][0] - heads["little"][0]).normalized()
        down = (heads["middle"][0] - wrist).normalized()
        palm = across.cross(down).normalized()
        if palm.x * outward > 0.0:
            palm = -palm
        base = hand.matrix @ hand.bone.matrix_local.inverted()
        for digit in DIGITS:
            h = heads[digit]
            if digit == "thumb":
                to = heads["index"][0] - h[1]
                d1 = (h[2] - h[1]).normalized()
                towards = (to - d1 * to.dot(d1)).normalized()
            else:
                towards = palm
            m = base
            for k in range(3):
                d = (h[min(k + 1, 2)] - h[min(k, 1)]).normalized()
                axis = d.cross(towards).normalized()
                turn = Matrix.Rotation(math.radians(bends[digit][k]), 4, axis)
                m = m @ Matrix.Translation(h[k]) @ turn @ Matrix.Translation(-h[k])
                pb = rig.pose.bones[bone(digit, k + 1, tag)]
                pb.matrix = m @ pb.bone.matrix_local
