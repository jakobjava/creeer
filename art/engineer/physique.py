"""The engineer's build (G1.3f; ADR-0026): an average man of normal weight, not the base mesh's
athlete (the user, 2026-10-21: "too muscular", "still way too buff, too broad", "the hip is
too wide"). On the fitted body (`body.py`, A-pose) his muscles' relief is smoothed away below
the neck, above the hands and feet; then he is measured and scaled, region by region, to one
group of measured men: ANSUR II's men of normal weight (BMI 18.5 to 25: 1,061 of 4,082; mean
BMI 22.9, 70.9 kg at 1.758 m). All of ANSUR's men (BMI 27.7) carry a belly, their waist wider
than their chest, which read as wide hips once the shoulders were narrow.

- the trunk across and through, height by height, to their breadths and depths at the seat,
  the waist and the chest; the shoulders' slope lowered until the neck's base has their girth
  (the base mesh's trapezius is a lifter's);
- the legs round their axes to their thigh and calf girths;
- the arms round their axes to their biceps girth (measured flexed, so an upper bound for an
  arm at rest), and moved in, the shoulders with them, until the breadth across the shoulders
  and upper arms is theirs.

All targets are at a stature of 1.80 m, scaled from the group's mean stature (×1.0236).
"""

import body as B
import garments
import numpy as np
from mathutils import Vector

# ANSUR II's men of normal weight at 1.80 m: buttock, waist (omphalion) and chest heights (all
# men's) with the trunk's breadth and depth there; thigh girth at the gluteal furrow and calf
# girth at its largest; biceps girth (flexed) and the breadth across the deltoids. (US
# civilians agree: NHANES 2015-2018's median upper arm, 34.4 cm, is all weights, at rest; the
# US median across the deltoids for men of 19 to 65, 47.0 cm in City University of Hong Kong's
# anthropometry tables, is 0.482 m at 1.80 m.)
GIRTHS = [(0.91, 0.332, 0.222), (1.083, 0.297, 0.208), (1.323, 0.282, 0.231)]
THIGH, CALF = (0.80, 0.574), (0.36, 0.372)
UPPER_ARM = 0.330
BIDELTOID = 0.490
# The base of the neck: the jugular notch (the fitted body's, in front of the neck) and the
# seventh cervical vertebra (ANSUR's cervicale height, behind it), and the group's girth there;
# the neck below the larynx, square to its axis, and its girth (only measured).
NECK_BASE = ((0.0, -0.065, 1.459), (0.0, 0.06, 1.555), 0.422)
NECK = ((0.0, -0.06, 1.51), (0.0, 0.055, 1.575), 0.382)
CROTCH = 0.867
RELIEF_PASSES = 120
FLATTEN_PASSES = 60


def girth(points):
    """A tape's measure round `points` (x, y): their convex hull's perimeter."""
    from mathutils.geometry import convex_hull_2d

    ring = [Vector(points[i]) for i in convex_hull_2d([tuple(p) for p in points])]
    return sum((a - b).length for a, b in zip(ring, ring[1:] + ring[:1]))


def smooth(a, b, x):
    t = np.clip((np.asarray(x, dtype=float) - a) / (b - a), 0.0, 1.0)
    return t * t * (3 - 2 * t)


class Body:
    """The skin's vertices and weights as arrays, written back by `store`."""

    def __init__(self, skin):
        self.mesh = skin.data
        w = B.weights(self.mesh)
        self.arm = {side: np.array(w[("arm", side)]) for side in (-1, 1)}
        self.arms = self.arm[1] + self.arm[-1]
        self.foot = np.array(w[("foot", 1)]) + np.array(w[("foot", -1)])
        self.load()

    def load(self):
        self.co = np.array([v.co[:] for v in self.mesh.vertices])

    def store(self):
        self.mesh.vertices.foreach_set("co", self.co.ravel())
        self.mesh.update()


def average(skin):
    """Makes `skin` (fitted, A-pose) an average man; returns what was measured and fitted."""
    b = Body(skin)
    flatten(b)
    report = {"upper arm": slim_arms(b), "neck": slim_neck(b)}
    report["shoulders lowered"] = lower_shoulders(b)
    report["arm in"] = arms_in(b)
    trunk(b, B.ARM_IN)
    legs(b)
    b.store()
    return report


def flatten(b):
    """The muscles' relief smoothed away: Taubin's passes everywhere below the neck but the hands
    and feet, then plain averaging over the trunk (Taubin's barely touch reliefs 20 cm wide,
    like the pectorals'). The trunk shrinks; its fit undoes that."""
    z = b.co[:, 2]
    below_neck = 1.0 - smooth(1.44, 1.5, z)
    hands = np.clip((b.arms - 0.5) * 2, 0, 1) * np.array([_beyond_wrist(c) for c in b.co])
    garments.soften(b.mesh, RELIEF_PASSES, where=below_neck * (1.0 - hands) * (1.0 - b.foot))
    trunk_only = smooth(0.95, 1.05, z) * below_neck * np.clip(1.0 - 2.0 * b.arms, 0, 1)
    garments.soften(b.mesh, FLATTEN_PASSES, shrink=0.0, where=trunk_only)
    b.load()


def _beyond_wrist(c):
    """1 past the wrist (on the hand), 0 above it, for a fitted A-pose point."""
    side = 1 if c[0] > 0 else -1
    w, e = B.joint("wrist", side), B.joint("elbow", side)
    return float(smooth(-0.01, 0.02, (Vector(c) - w).dot((w - e).normalized())))


def _axis_points(co, side):
    """The nearest points on the arm's axis (shoulder–elbow–wrist, fitted) to `co`."""
    s, e, w = (np.array(B.joint(n, side)) for n in ("shoulder", "elbow", "wrist"))
    best, dist = None, None
    for a, c in ((s, e), (e, w)):
        d = c - a
        t = np.clip(((co - a) @ d) / (d @ d), 0.0, 1.0)
        p = a + t[:, None] * d
        r = np.linalg.norm(co - p, axis=1)
        if best is None:
            best, dist = p, r
        else:
            closer = r < dist
            best[closer], dist[closer] = p[closer], r[closer]
    return best


def upper_arm_girth(b, side):
    """The arm's girth halfway from the shoulder to the elbow, across its axis."""
    s, e = B.joint("shoulder", side), B.joint("elbow", side)
    mid, axis = (s + e) / 2, (e - s).normalized()
    across = axis.cross(Vector((0, 1, 0))).normalized()
    up = axis.cross(across)
    d = (b.co - np.array(mid)) @ np.array(axis)
    pts = b.co[(np.abs(d) < 0.005) & (b.arm[side] > 0.5)] - np.array(mid)
    return girth(np.stack([pts @ np.array(across), pts @ np.array(up)], axis=1))


def slim_arms(b):
    """The arms round their axes to NHANES' upper-arm girth, hands aside."""
    measured = {side: upper_arm_girth(b, side) for side in (-1, 1)}
    hands = np.array([_beyond_wrist(c) for c in b.co])
    for side in (-1, 1):
        scale = UPPER_ARM / measured[side]
        on = b.arm[side] * (1.0 - hands)
        p = _axis_points(b.co, side)
        b.co += (on * (scale - 1.0))[:, None] * (b.co - p)
    return {side: (measured[side], upper_arm_girth(b, side)) for side in (-1, 1)}


def slim_neck(b):
    """The neck round its axis to the group's girth below the larynx, fading out into the head
    above and the shoulders below and outward. Returns the girths before and after."""
    before = neck_base(b.co, NECK)
    front, back = (np.array(p) for p in NECK[:2])
    centre = (front + back) / 2
    r = np.hypot(b.co[:, 0] - centre[0], b.co[:, 1] - centre[1])
    z = b.co[:, 2]
    # Not the chin and jaw: nothing forward of the neck's front.
    jaw = smooth(-0.09, -0.07, b.co[:, 1])
    region = (1 - smooth(0.08, 0.13, r)) * smooth(1.42, 1.48, z) * (1 - smooth(1.56, 1.6, z)) * jaw * (b.arms < 0.5)
    f = NECK[2] / before
    for _ in range(3):
        scale = 1 + (f - 1) * region
        co = b.co.copy()
        co[:, 0] = centre[0] + (co[:, 0] - centre[0]) * scale
        co[:, 1] = centre[1] + (co[:, 1] - centre[1]) * scale
        f *= NECK[2] / neck_base(co, NECK)
    b.co = co
    return before, neck_base(b.co, NECK)


def neck_base(co, where=None):
    """The girth round the base of the neck, as ANSUR's tape: over the jugular notch and the
    seventh cervical vertebra (the plane through both, square to the midline), skin within
    0.2 m of the middle; or round the plane `where` (front, back, …) gives."""
    (front, back) = (np.array(p) for p in (where or NECK_BASE)[:2])
    d = back - front
    n = np.array([0.0, -d[2], d[1]]) / np.hypot(d[1], d[2])
    pts = co[(np.abs((co - front) @ n) < 0.004) & (np.abs(co[:, 0]) < 0.2)]
    v = np.cross(n, [1.0, 0.0, 0.0])
    return girth(np.stack([pts[:, 0], pts @ v], axis=1))


def lower_shoulders(b):
    """The shoulders' slope from the neck brought down (a lifter's trapezius taken off) until the
    neck's base measures ANSUR's girth: a drop greatest where the neck meets the shoulders,
    none at the neck or out at the shoulder's point, fading in above the chest and out below
    the jaw. Returns the drop (m) and the girths before and after."""
    ax, z = np.abs(b.co[:, 0]), b.co[:, 2]
    shape = smooth(0.045, 0.09, ax) * (1 - smooth(0.13, 0.21, ax)) * smooth(1.40, 1.47, z) * (1 - smooth(1.53, 1.56, z))
    before = neck_base(b.co)

    def dropped(d):
        co = b.co.copy()
        co[:, 2] -= d * shape
        return co

    lo, hi = 0.0, 0.05
    if neck_base(dropped(hi)) > NECK_BASE[2]:
        lo = hi
    for _ in range(20):
        mid = (lo + hi) / 2
        lo, hi = (mid, hi) if neck_base(dropped(mid)) > NECK_BASE[2] else (lo, mid)
    b.co = dropped(lo)
    return lo, before, neck_base(b.co)


def arms_in(b):
    """How far the arms move in for the breadth across the shoulders and upper arms, hanging, to
    be the target's: measured on the upper arms posed as they will be."""
    reach = 0.0
    for side in (-1, 1):
        s, e = (np.array(B.joint(n, side)) for n in ("shoulder", "elbow"))
        along = (b.co - s) @ (e - s) / ((e - s) @ (e - s))
        upper = b.co[(b.arm[side] > 0.5) & (along < 0.8)]
        posed = [B.posed(Vector(c), side, "arm") for c in upper]
        reach = max(reach, max(side * p.x for p in posed))
    return max(0.0, reach - BIDELTOID / 2)


def trunk(b, arm_in):
    """The trunk scaled across and through to the targets, height by height; the shoulders
    narrowed with the arms moved in, the arms moved in."""
    co, out = b.co, b.co.copy()
    on_trunk = (b.arms < 0.5) & (co[:, 2] > CROTCH - 0.06)
    knots = []
    for z, breadth, depth in GIRTHS:
        near = on_trunk & (np.abs(co[:, 2] - z) < 0.006)
        xs, ys = co[near, 0], co[near, 1]
        knots.append((z, breadth / np.ptp(xs), depth / np.ptp(ys), (ys.max() + ys.min()) / 2))
    s = B.joint("shoulder", 1)
    knots.append((s.z, (s.x - arm_in) / s.x, 1.0, knots[-1][3]))
    t = (1.0 - np.minimum(b.arms, 1.0)) * smooth(CROTCH - 0.06, CROTCH, co[:, 2])
    for i, (x, y, z) in enumerate(co):
        sx, sy, cy = _between(knots, z)
        out[i, 0] = x * (1 + t[i] * (sx - 1))
        out[i, 1] = cy + (y - cy) * (1 + t[i] * (sy - 1))
    for side in (-1, 1):
        out[:, 0] -= side * arm_in * b.arm[side]
    b.co = out
    B.ARM_IN = arm_in


def _between(knots, z):
    """The trunk's scales at height `z`: between the knots; below the first fading to none over
    0.1 m, above the last (the shoulders) over 0.04 m, below the neck."""
    zs = [k[0] for k in knots]
    if z <= zs[0]:
        f = float(smooth(zs[0] - 0.1, zs[0], z))
        return 1 + f * (knots[0][1] - 1), 1 + f * (knots[0][2] - 1), knots[0][3]
    if z >= zs[-1]:
        f = 1.0 - float(smooth(zs[-1], zs[-1] + 0.04, z))
        return 1 + f * (knots[-1][1] - 1), 1 + f * (knots[-1][2] - 1), knots[-1][3]
    k = next(i for i in range(len(zs) - 1) if z <= zs[i + 1])
    t = float(smooth(zs[k], zs[k + 1], z))
    return tuple(a + t * (c - a) for a, c in zip(knots[k][1:], knots[k + 1][1:]))


def legs(b, passes=2):
    """The legs round their axes (each 1 cm band's middle) to the thigh's and calf's girths,
    fading out into the hips and the ankles; a second pass takes up what the first missed (the
    bands' middles are not quite the girths')."""
    for _ in range(passes):
        _legs(b)


def _legs(b):
    co = b.co
    leg = (b.arms < 0.5) & (co[:, 0] > 0.02) & (co[:, 2] < CROTCH)
    scales = {}
    for name, (z, target) in (("thigh", THIGH), ("calf", CALF)):
        scales[name] = (z, target / girth(co[leg & (np.abs(co[:, 2] - z) < 0.006), :2]))
    (z1, s1), (z2, s2) = scales["calf"], scales["thigh"]

    def scale(z):
        if z >= z2:
            return s2
        if z <= z1:
            return 1 + (s1 - 1) * float(smooth(0.1, z1, z))
        return s1 + (s2 - s1) * float(smooth(z1, z2, z))

    for side in (-1, 1):
        leg = (b.arms < 0.5) & (co[:, 0] * side > 0.02) & (co[:, 2] < CROTCH)
        for z0 in np.arange(0.1, CROTCH + 0.01, 0.01):
            near = leg & (co[:, 2] >= z0 - 0.005) & (co[:, 2] < z0 + 0.005)
            if not near.any():
                continue
            centre = co[near, :2].mean(axis=0)
            f = 1 + (scale(z0) - 1) * (1 - float(smooth(CROTCH - 0.06, CROTCH, z0)))
            co[near, :2] = centre + (co[near, :2] - centre) * f


def measure(skin):
    """The posed skin's measures against the targets, as `art:` lines."""
    b = Body(skin)
    co, lines = b.co, []
    on_trunk = b.arms < 0.5
    for z, breadth, depth in GIRTHS:
        near = on_trunk & (np.abs(co[:, 2] - z) < 0.006)
        lines.append(f"at {z} m: {np.ptp(co[near, 0]):.3f} × {np.ptp(co[near, 1]):.3f} (target {breadth} × {depth})")
    lines.append(f"neck's base girth {neck_base(co):.3f} (target {NECK_BASE[2]})")
    lines.append(f"neck girth {neck_base(co, NECK):.3f} (theirs {NECK[2]})")
    band = (co[:, 2] > 1.25) & (co[:, 2] < 1.5)
    lines.append(f"across the deltoids {2 * np.abs(co[band, 0]).max():.3f} (target {BIDELTOID:.3f})")
    for name, (z, target) in (("thigh", THIGH), ("calf", CALF)):
        near = on_trunk & (co[:, 0] > 0.02) & (np.abs(co[:, 2] - z) < 0.006)
        lines.append(f"{name} girth {girth(co[near, :2]):.3f} (target {target})")
    for line in lines:
        print("art: " + line)
    return lines
