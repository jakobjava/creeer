"""The ragdoll's segments measured on the engineer (G1.3f; ADR-0023 amended, ADR-0026): run in
the live session after `blockout.py` has built the scene; prints the table that
`creeer_sim::character::ragdoll` keeps (`SEGMENTS`, `LINKS`'s places), and how far the body
strays from it.

    exec(open("art/engineer/segments.py").read())

Each vertex of the base figure's skin is given to a segment: by its weights to an arm or a
leg, split at the joints (the thigh from the pelvis by de Leva's boundary, a plane through the
hip joint in front and the gluteal fold behind); the rest to the head (above the neck's base),
the pelvis (below the back joint, at the waist) or the torso. A limb's box runs from joint to
joint; across, it is the limb's measured section (its extremes from the bone between 20 % and
85 % of the way, the upper arm's from 10 %) as a box of the same area: an ellipse's half
widths times √π/2. The trunk's and the feet's likewise from their extents. Heights are on the
boots' 3 cm soles.
"""

import math
import pathlib
import sys

import bpy
import numpy as np

try:
    HERE = pathlib.Path(__file__).resolve().parent
except NameError:  # run by exec() in a live session
    HERE = pathlib.Path(r"C:\Users\jakmo\dev\creëer\art\engineer")
sys.path.insert(0, str(HERE))
import body as B  # noqa: E402

# Not reloaded: the fit `blockout.py` just made (its arms moved in) is the one measured.

SOLE = 0.03
AREA = math.sqrt(math.pi) / 2
NAMES = ["pelvis", "torso", "head"] + [
    f"{limb}.{tag}" for tag in "RL" for limb in ("upper_arm", "forearm", "hand", "thigh", "shank", "foot")
]
WAIST = 1.083  # the back joint: ANSUR's omphalion height at 1.80 m
# The neck's base (head above), and de Leva's pelvis-thigh boundary (pelvis above): planes
# through a point in front and one behind, on the midline (the skin's frame, m).
NECK_BASE = ((0.0, -0.06, 1.505), (0.0, 0.06, 1.555))
GLUTEAL = ((0.0, -0.11, 0.93), (0.0, 0.07, 0.82))
FROM = {"upper_arm": 0.1, "forearm": 0.2, "hand": 0.2, "thigh": 0.2, "shank": 0.2}


def joint(name, side, limb):
    return np.array(B.posed(B.joint(name, side), side, limb))


def game(p):
    """Blender's axes (z up, facing −y) to the game's (y up, z forward), on the soles."""
    return np.array([p[0], p[2] + SOLE, -p[1]])


def above(co, plane):
    front, back = (np.array(p) for p in plane)
    d = back - front
    n = np.array([0.0, -d[2], d[1]])
    n = n / np.linalg.norm(n) * (1 if d[1] > 0 else -1)
    return (co - front) @ n > 0


def segments(co, weights):
    seg = np.full(len(co), -1)
    z = co[:, 2]
    for side, tag in ((-1, "R"), (1, "L")):
        arm = np.array(weights[("arm", side)]) > 0.5
        leg = (np.array(weights[("leg", side)]) + np.array(weights[("foot", side)]) > 0.5) & ~arm
        s, e = joint("shoulder", side, "arm"), joint("elbow", side, "arm")
        w, f = joint("wrist", side, "forearm"), joint("fingertip", side, "forearm")
        past = lambda a, b: (co - a) @ ((b - a) / np.linalg.norm(b - a)) > 0  # noqa: E731
        hand = arm & past(w, f)
        forearm = arm & ~hand & past(e, w)
        seg[arm & ~hand & ~forearm] = NAMES.index(f"upper_arm.{tag}")
        seg[forearm] = NAMES.index(f"forearm.{tag}")
        seg[hand] = NAMES.index(f"hand.{tag}")
        k, a = joint("knee", side, "leg"), joint("foot", side, "foot")
        foot = leg & (z < a[2] + 0.02)
        shank = leg & ~foot & (z < k[2])
        thigh = leg & ~foot & ~shank & ~above(co, GLUTEAL)
        seg[foot] = NAMES.index(f"foot.{tag}")
        seg[shank] = NAMES.index(f"shank.{tag}")
        seg[thigh] = NAMES.index(f"thigh.{tag}")
    rest = seg < 0
    head = rest & above(co, NECK_BASE)
    pelvis = rest & ~head & (z < WAIST)
    seg[head], seg[pelvis], seg[rest & ~head & ~pelvis] = 2, 0, 1
    return seg


def boxes(co, seg):
    out = {}
    for name in ("pelvis", "torso", "head"):
        pts = co[seg == NAMES.index(name)]
        lo, hi = np.sort(np.stack([game(pts.min(axis=0)), game(pts.max(axis=0))]), axis=0)
        out[name] = ((lo + hi) / 2, (hi - lo) / 2 * np.array([AREA, 1.0, AREA]))
    for side, tag in ((-1, "R"), (1, "L")):
        bones = {
            "upper_arm": (joint("shoulder", side, "arm"), joint("elbow", side, "arm")),
            "forearm": (joint("elbow", side, "arm"), joint("wrist", side, "forearm")),
            "hand": (joint("wrist", side, "forearm"), joint("fingertip", side, "forearm")),
            "thigh": (joint("hip", side, "leg"), joint("knee", side, "leg")),
            "shank": (joint("knee", side, "leg"), joint("foot", side, "foot")),
        }
        for limb, (a, b) in bones.items():
            pts = co[seg == NAMES.index(f"{limb}.{tag}")]
            length = np.linalg.norm(b - a)
            u = (b - a) / length
            t = (pts - a) @ u / length
            off = pts - (a + np.outer(t * length, u))
            keep = (t > FROM[limb]) & (t < 0.85) & (np.linalg.norm(off, axis=1) < 0.11)
            lo, hi = off[keep].min(axis=0), off[keep].max(axis=0)
            mid = (a + b) / 2 + np.array([(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, 0.0])
            half = np.array([(hi[0] - lo[0]) / 2 * AREA, length / 2, (hi[1] - lo[1]) / 2 * AREA])
            out[f"{limb}.{tag}"] = (game(mid), half)
        pts = co[seg == NAMES.index(f"foot.{tag}")]
        lo, hi = np.sort(np.stack([game(pts.min(axis=0)), game(pts.max(axis=0))]), axis=0)
        lo[1] = 0.0  # down to the sole
        out[f"foot.{tag}"] = ((lo + hi) / 2, (hi - lo) / 2 * np.array([AREA, 1.0, 1.0]))
    return out


def measure(scene=None):
    scene = scene or bpy.context.scene
    skin = scene.objects["base body skin"]
    # Its mesh stands on the boots' soles; measured here from its bare feet, as the fit is.
    co = np.array([v.co[:] for v in skin.data.vertices]) - [0.0, 0.0, SOLE]
    seg = segments(co, B.weights(skin.data))
    table = boxes(co, seg)
    g = np.stack([co[:, 0], co[:, 2] + SOLE, -co[:, 1]], axis=1)
    for k, name in enumerate(NAMES):
        c, h = table[name]
        out = np.linalg.norm(np.maximum(np.abs(g[seg == k] - c) - h, 0.0), axis=1)
        print(
            f"art: {name:12s} centre ({c[0]:+.3f}, {c[1]:.3f}, {c[2]:+.3f}) half ({h[0]:.3f}, {h[1]:.3f}, {h[2]:.3f})"
            f"  beyond 3 cm: {np.mean(out > 0.03) * 100:4.1f} %, worst {out.max():.3f}"
        )
    for side, tag in ((-1, "R"), (1, "L")):
        for name, (j, limb) in {
            "shoulder": ("shoulder", "arm"),
            "elbow": ("elbow", "arm"),
            "wrist": ("wrist", "forearm"),
            "hip": ("hip", "leg"),
            "knee": ("knee", "leg"),
            "ankle": ("foot", "foot"),
        }.items():
            p = game(joint(j, side, limb))
            print(f"art: {name}.{tag} at ({p[0]:+.3f}, {p[1]:.3f}, {p[2]:+.3f})")
    neck = (NECK_BASE[0][2] + NECK_BASE[1][2]) / 2 + SOLE
    print(f"art: back at (0, {WAIST + SOLE:.3f}, 0); neck at (0, {neck:.3f}, 0), the neck's base midway")
    return table


measure()
