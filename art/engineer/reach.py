"""How far the ragdoll reaches at its joints' limits (G1.3f, `check_fit.py`). Its arms collide
with its pelvis, torso and head, and nothing else collides (`creeer_sim::character::ragdoll::
pass_within`): a joint turned to its limit that puts an arm's box into the core's is a pose the
ragdoll never takes. The arm stops where the boxes meet, and that is the pose to check. Swung
across the chest to the shoulder's cone, an arm drew the neckline 12 mm into the neck."""

import json
import pathlib

from mathutils import Vector

BONES = json.loads((pathlib.Path(__file__).resolve().parent / "bones.json").read_text(encoding="utf-8"))
ARMS = tuple(f"{s}.{t}" for s in ("upper_arm", "forearm", "hand") for t in "LR")
CORE = ("pelvis", "torso", "head")


def g2b(p):
    return Vector((p[0], -p[2], p[1]))


def boxes(rig):
    """Each segment's box as `rig` is posed: its middle, its axes and its half sizes."""
    out = {}
    for seg in BONES["segments"]:
        pb = rig.pose.bones[seg["name"]]
        move = pb.matrix @ pb.bone.matrix_local.inverted()
        turn = move.to_3x3()
        h = seg["half"]
        out[seg["name"]] = (move @ g2b(seg["centre"]), [turn @ Vector(a) for a in ((1, 0, 0), (0, 1, 0), (0, 0, 1))], (h[0], h[2], h[1]))
    return out


def apart(a, b):
    """Whether the boxes `a` and `b` are apart: an axis separates them (the separating axis
    test: their faces' normals, and the cross products of their edges)."""
    (ca, ua, ha), (cb, ub, hb) = a, b
    d = cb - ca
    for axis in [*ua, *ub, *(x.cross(y) for x in ua for y in ub)]:
        if axis.length < 1e-9:
            continue
        axis = axis.normalized()
        ra = sum(h * abs(u.dot(axis)) for u, h in zip(ua, ha))
        rb = sum(h * abs(u.dot(axis)) for u, h in zip(ub, hb))
        if abs(d.dot(axis)) > ra + rb:
            return True
    return False


def meets(rig):
    """Whether an arm's box meets the core's as `rig` is posed."""
    b = boxes(rig)
    return any(not apart(b[arm], b[core]) for arm in ARMS for core in CORE)
