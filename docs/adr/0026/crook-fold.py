"""Where dual-quaternion skinning folds the elbow's crook over itself at full flexion (G1.3f,
ADR-0026's "The engineer's joints closed fully"): why edge loops could not part its layers.
In the bend plane, a point at s along the arm (from the elbow's axis) and d off it toward the
crook, turned by phi(w(s)) about the axis, maps with Jacobian determinant 1 - phi'(s) d: the
skinning folds wherever phi'(s) d > 1, whatever the mesh's density.
blender -b --factory-startup --python crook-fold.py -- <blend>"""
import math
import pathlib
import sys

import bpy
from mathutils import Vector

ART = pathlib.Path(__file__).resolve().parents[3] / "art" / "engineer"
sys.path.insert(0, str(ART))
import check_fit as C  # noqa: E402

blend = sys.argv[sys.argv.index("--") + 1]
bpy.ops.wm.open_mainfile(filepath=blend)
joint = next(j for j in C.BONES["joints"] if j["name"] == "elbow.L")
pivot = C.g2b(joint["at"])
axis = C.g2b(joint["axis"]).normalized()
theta = 2.5
up = (C.g2b(next(j for j in C.BONES["joints"] if j["name"] == "shoulder.L")["at"]) - pivot).normalized()
# The crook's side: where the forearm turns to (the posed hand's direction at the limit).
crook = up.cross(axis).normalized()
turned = Vector((0, 0, -1))
turned.rotate(__import__("mathutils").Matrix.Rotation(-theta, 3, axis))
if crook.dot(turned) < 0:
    crook = -crook


def phi(w):
    """The angle the dual-quaternion blend of the upper arm (still) and the forearm (turned by
    theta) gives a point weighted w to the forearm."""
    return 2.0 * math.atan2(w * math.sin(theta / 2), (1 - w) + w * math.cos(theta / 2))


for piece in ("body", "base_layer", "coverall", "jacket"):
    for obj in bpy.data.objects:
        if not (obj.type == "MESH" and obj.name.startswith(piece + ":") and obj.name.endswith("@0") and ("upper_arm.L" in obj.name or "forearm.L" in obj.name)):
            continue
        names = {g.index: g.name for g in obj.vertex_groups}
        fw = {}
        for v in obj.data.vertices:
            fw[v.index] = sum(g.weight for g in v.groups if names[g.group] == "forearm.L")
        mesh = obj.data
        nbrs = {v.index: set() for v in mesh.vertices}
        for e in mesh.edges:
            a, b = e.vertices
            nbrs[a].add(b)
            nbrs[b].add(a)
        folded = band = 0
        worst = 0.0
        for v in mesh.vertices:
            rel = v.co - pivot
            s = rel.dot(-up)
            d = rel.dot(crook)
            if abs(s) > 0.08 or d < 0.0 or abs(rel.dot(axis)) > 0.06:
                continue
            band += 1
            # phi' along the arm, from neighbours: least squares of dphi against ds.
            num = den = 0.0
            for u in nbrs[v.index]:
                du = (mesh.vertices[u].co - v.co).dot(-up)
                num += (phi(fw[u]) - phi(fw[v.index])) * du
                den += du * du
            if den <= 0:
                continue
            slope = -num / den  # turning toward the crook as s grows down the forearm
            k = abs(slope) * d
            worst = max(worst, k)
            folded += k > 1.0
        print(f"art: fold: {obj.name}: {band} points in the crook's band, {folded} folded (phi' d > 1), worst phi' d {worst:.2f}")
