"""The experiments on the engineer's joints closed fully (G1.3f, ADR-0026's "The engineer's
joints closed fully"): the engineer's close-up level, unbaked (the fit needs no colours), built by
the scripts as they were at commit 7428122 with changes patched in by environment variables, and
saved to a blend for `check_fit.py`:

    blender -b --factory-startup --python joint-experiments.py -- <out.blend> <full|base>

- `full`: nothing decimated (439,890 triangles: edge loops at their limit);
- `WAIST=<m>`: the thighs' weight given way to the pelvis's above the hip joints over that;
- `INSIDE=1`: the faces a wearable over them encloses hidden (`in.<wearable>`), as `CONE`
  (degrees), `INSET` (m beneath the point), `ADJ=1` (the wearable over it on its limb);
- `KNEE=<share>`: the knee pads that much the thigh's; `CUFF=1`: the coverall's cuffs the
  forearms'; `STRAP=shank`: the knee straps the shank's.

Kept for the record; the scripts have since changed (`art/engineer/inside.py` is what was taken).
"""

import pathlib
import sys

import bpy

ART = pathlib.Path(__file__).resolve().parents[3] / "art"
sys.path.insert(0, str(ART))
sys.path.insert(0, str(ART / "engineer"))
import common  # noqa: E402

args = sys.argv[sys.argv.index("--") + 1:]
out, variant = pathlib.Path(args[0]), args[1]
common.reset()
import models  # noqa: E402
import bake  # noqa: E402

models.LEVELS = (1.0,)
bake.bake = lambda obj, **kw: None
models.per_vertex = lambda obj: None
models.Sf.wear = models.Sf.stains = lambda *a: None
models.Sf.hair_grain = lambda *a: None
import os  # noqa: E402

import body as B  # noqa: E402

WAIST = float(os.environ.get("WAIST", "0"))
if WAIST:
    weigh = B.weigh_segments

    def weigh_segments(body):
        """Above the hip joint each thigh's weight gives way to the pelvis's over WAIST (m)."""
        weigh(body)
        pelvis = body.data.attributes["s.pelvis"]
        for side, tag in ((1, "L"), (-1, "R")):
            hip = B.joint("hip", side).z
            thigh = body.data.attributes[f"s.thigh.{tag}"]
            for v in body.data.vertices:
                t = min(max((v.co.z - hip) / WAIST, 0.0), 1.0)
                moved = thigh.data[v.index].value * t * t * (3.0 - 2.0 * t)
                thigh.data[v.index].value -= moved
                pelvis.data[v.index].value += moved

    B.weigh_segments = weigh_segments
import skinning as S  # noqa: E402
import regions as Rg  # noqa: E402
from mathutils.bvhtree import BVHTree  # noqa: E402
from mathutils.kdtree import KDTree  # noqa: E402

KNEE = float(os.environ.get("KNEE", "0"))
CUFF = os.environ.get("CUFF") == "1"
MARGIN = float(os.environ.get("INSIDE", "0"))
COVERS = {"body": ("base_layer",), "coverall": ("jacket", "boots"), "gloves": ("coverall",)}
state = {}
complete = S.complete_weights


def complete_weights(pieces, skin):
    complete(pieces, skin)
    state["pieces"] = pieces
    if KNEE:
        for obj in pieces["knee_pads"]:
            if obj.name.startswith("knee pad"):
                tag = "L" if sum(v.co.x for v in obj.data.vertices) > 0 else "R"
                n = len(obj.data.vertices)
                S.give_weights(obj, {f"thigh.{tag}": [KNEE] * n, f"shank.{tag}": [1 - KNEE] * n})
    if os.environ.get("STRAP") == "shank":
        # The knee pads' straps and buckles round the calf's top: the shank's.
        for obj in pieces["knee_pads"]:
            if not obj.name.startswith("knee pad"):
                tag = "L" if sum(v.co.x for v in obj.data.vertices) > 0 else "R"
                S.give_weights(obj, {f"shank.{tag}": [1.0] * len(obj.data.vertices)})
    if CUFF:
        # The coverall's sleeves held by the forearms alone: the hand turns inside the cuff.
        for obj in pieces["coverall"]:
            if "s.pelvis" not in obj.data.attributes:
                continue
            w = S.segment_weights(obj)
            for tag in "LR":
                for i, h in enumerate(w[f"hand.{tag}"]):
                    w[f"forearm.{tag}"][i] += h
                    w[f"hand.{tag}"][i] = 0.0
            S.give_weights(obj, w)


S.complete_weights = complete_weights
covering = {}


def cover(w):
    if w not in covering:
        scene = bpy.context.scene
        cloth = [o for o in state["pieces"][w] if not o.get("hardware")]
        obj = models.join(scene, "cover." + w, cloth)
        tree = BVHTree.FromObject(obj, bpy.context.evaluated_depsgraph_get())
        rim = models.open_edges(obj)
        kd = KDTree(len(rim))
        for i in rim:
            kd.insert(obj.data.vertices[i].co, i)
        kd.balance()
        covering[w] = (tree, kd, segments(obj))
    return covering[w]


regions = Rg.face_regions


def skin_tree():
    if "skin" not in covering:
        skin = next(o for o in state["pieces"]["body"] if o.name == "skin")
        covering["skin"] = BVHTree.FromObject(skin, bpy.context.evaluated_depsgraph_get())
    return covering["skin"]
import math  # noqa: E402

from mathutils import Matrix  # noqa: E402

INSET = float(os.environ.get("INSET", "0.0001"))
CONE = math.radians(float(os.environ.get("CONE", "60")))


def enclosed(tree, p, n):
    """Whether every line of sight from `p` within CONE of its normal `n` meets `tree` (the
    wearable over it): it lies inside it, not by an opening."""
    t = n.orthogonal().normalized()
    ways = [n]
    for k in range(8):
        axis = Matrix.Rotation(k * math.pi / 4, 3, n) @ t
        ways += [Matrix.Rotation(a, 3, axis) @ n for a in (CONE / 2, CONE)]
    start = p - n * INSET
    return all(tree.ray_cast(start, w, 0.5)[0] is not None for w in ways)


ROOTS = ("thigh.L", "thigh.R", "upper_arm.L", "upper_arm.R")


def segments(obj):
    """Each face's segment: the one its corners weigh most on."""
    weights = S.segment_weights(obj)
    names = list(weights)
    return [max(names, key=lambda n: sum(weights[n][v] for v in f.vertices)) for f in obj.data.polygons]


def adjacent(a, b):
    """Whether faces on segments `a` and `b` can lie one over the other: on one limb, or a
    limb's root against the trunk (the hips under the hem; not a hand beside the thigh)."""
    la, lb = S.limb(a), S.limb(b)
    return la == lb or (la == "trunk" and b in ROOTS) or (lb == "trunk" and a in ROOTS)


def face_regions(obj, piece, openings):
    out = regions(obj, piece, openings)
    if not MARGIN:
        return out
    own = segments(obj)
    vs = obj.data.vertices
    for w in COVERS.get(piece, ()):
        tree, rim, theirs = cover(w)
        inside = {}

        def corner(i):
            if i not in inside:
                v = vs[i]
                # Out as the check looks out: the skin's normal (a crease's own tilts).
                out = v.normal if piece == "body" else skin_tree().find_nearest(v.co)[1]
                hit, _, face, _ = tree.ray_cast(v.co + out * 1e-4, out, 0.06)
                near = hit is not None and (os.environ.get("ADJ") != "1" or adjacent(own_v[i], theirs[face]))
                inside[i] = near and enclosed(tree, v.co, out)
            return inside[i]

        weights = S.segment_weights(obj)
        names = list(weights)
        own_v = [max(names, key=lambda n: weights[n][i]) for i in range(len(vs))]
        for f in obj.data.polygons:
            if out[f.index] in ("hair", "face", "head", "neck") or out[f.index].startswith("in."):
                continue
            if all(corner(i) for i in f.vertices):
                out[f.index] = "in." + w
    return out


Rg.face_regions = face_regions
if variant == "full":
    for k in models.BUDGET:
        models.BUDGET[k] = 10 ** 8
try:
    parts = models.build()
    bpy.ops.wm.save_as_mainfile(filepath=str(out.resolve()))
except Exception:
    import traceback

    traceback.print_exc()
    sys.exit(1)
