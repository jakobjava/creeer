"""The engineer's blockout (G1.3f; ADR-0026 decision 8; docs/art-style.md §8): a real person's
body and the starting outfit, to judge the look before the detailed models (approved
2026-10-21). Two figures side by side: the base body in its base layer (a vest and shorts),
barefoot; and the starting outfit (a quilted coverall, a jacket in the player's colour with a
stand collar and a belt, leather knee and elbow pads, thick gloves, boots on rubber soles, and
goggles pushed up on the forehead). The head is bare, with short hair. The jetpack is not part
of it: the player makes it later. The rough guide is Factorio's FFF-40 engineer, without its
hard hat, tools and rucksack.

The body is Blender Studio's realistic man (Human Base Meshes, CC0), fitted to ANSUR II at
1.80 m and posed standing (`body.py`), an average man of normal weight (`physique.py`); the
pieces are built by `outfit.py`, every garment an offset of the body (`garments.py`). The
ragdoll's segments (bones.json, written from the Rust code) are drawn over the outfit as wire
boxes.

Not a model: nothing is exported. Run it in a Blender session (it builds into its own scene,
"engineer blockout", and leaves the others alone):

    exec(open("art/engineer/blockout.py").read())
"""

import importlib
import json
import pathlib
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

try:
    HERE = pathlib.Path(__file__).resolve().parent
except NameError:  # run by exec() in a live session
    HERE = pathlib.Path(r"C:\Users\jakmo\dev\creëer\art\engineer")
for path in (HERE, HERE.parent):
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))
import body as B  # noqa: E402
import garments as G  # noqa: E402
import outfit as O  # noqa: E402
import physique as P  # noqa: E402

# A live session keeps modules between runs: the files as they are now.
B, G = importlib.reload(B), importlib.reload(G)
P, O = importlib.reload(P), importlib.reload(O)

BONES = json.loads((HERE / "bones.json").read_text(encoding="utf-8"))
SCENE = "engineer blockout"
# The figures, side by side; the base body barefoot (its pieces stand on the boots' soles).
FIGURES = {"base": (-0.6, -O.SOLE), "outfit": (0.6, 0.0)}
WORN = {"base": ("body", "base_layer"), "outfit": ("body", "coverall", "jacket", "belt", "gloves", "boots", "knee_pads", "elbow_pads", "goggles")}


def g(x, y, z):
    """The game's axes (y up, z forward) in Blender's (z up, the figure facing −y)."""
    return Vector((x, -z, y))


def ragdoll_boxes(scene, offset):
    """The ragdoll's segments as wire boxes, where the physics has them."""
    for s in BONES["segments"]:
        bm = bmesh.new()
        bmesh.ops.create_cube(bm, size=2.0)
        (x, y, z), (hx, hy, hz) = s["centre"], s["half"]
        bmesh.ops.transform(
            bm,
            matrix=Matrix.Translation(g(x, y, z) + Vector((offset, 0, 0))) @ Matrix.Diagonal((hx, hz, hy, 1.0)),
            verts=bm.verts,
        )
        mesh = bpy.data.meshes.new("ragdoll " + s["name"])
        bm.to_mesh(mesh)
        bm.free()
        obj = bpy.data.objects.new("ragdoll " + s["name"], mesh)
        obj.display_type = "WIRE"
        obj.hide_render = True
        scene.collection.objects.link(obj)


def build():
    scene = bpy.data.scenes.get(SCENE) or bpy.data.scenes.new(SCENE)
    for obj in list(scene.objects):
        bpy.data.objects.remove(obj)
    # Weighting, joining and applying modifiers work in the active scene's view layer.
    if bpy.context.window is not None:
        bpy.context.window.scene = scene
    skin, eyes, report = O.fitted(scene)
    for name, value in report.items():
        print(f"art: {name}: {value}")
    pieces = O.build(scene, skin, eyes)
    for obj in (skin, *eyes):
        bpy.data.objects.remove(obj)
    for figure, (x, z) in FIGURES.items():
        for piece in WORN[figure]:
            for src in pieces[piece]:
                obj = src.copy()
                obj.data = src.data.copy()
                scene.collection.objects.link(obj)
                obj.name = f"{figure} {piece} {src.name}"
                obj.location = (x, 0.0, z)
    for objs in pieces.values():
        for obj in objs:
            bpy.data.objects.remove(obj)
    P.measure(scene.objects["base body skin"], lift=O.SOLE)
    ragdoll_boxes(scene, 0.6)
    return scene


build()
