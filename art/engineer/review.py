"""The engineer in detail, live (G1.3f): the starting outfit as `models.py` takes it (built by
`outfit.py`, detailed by `detail.py`) at full resolution, before its levels are made, to judge
the detail while it is made. The base layer is left out, the body's skin kept for the head and
neck. Run it in a Blender session (it builds into its own scene, "engineer review"):

    exec(open("art/engineer/review.py").read())

The levels themselves, baked, are judged in the exported model (`cargo xtask art --blend
engineer`, `target/art/engineer.blend`).
"""

import importlib
import pathlib
import sys

import bpy

try:
    HERE = pathlib.Path(__file__).resolve().parent
except NameError:  # run by exec() in a live session
    HERE = pathlib.Path(r"C:\Users\jakmo\dev\creëer\art\engineer")
for path in (HERE, HERE.parent):
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))
import body as B  # noqa: E402
import boots  # noqa: E402
import detail as D  # noqa: E402
import gear  # noqa: E402
import hair  # noqa: E402
import last  # noqa: E402
import garments as G  # noqa: E402
import outfit as O  # noqa: E402
import models as M  # noqa: E402
import physique as P  # noqa: E402
import relief as R  # noqa: E402
import trims as T  # noqa: E402

# A live session keeps modules between runs: the files as they are now.
B, G, P, O = (importlib.reload(m) for m in (B, G, P, O))
R, T, last, D, gear, boots, M, hair = (importlib.reload(m) for m in (R, T, last, D, gear, boots, M, hair))

SCENE = "engineer review"


def refresh():
    """The session's token materials made anew, in the palette's colours as they are now."""
    import common

    for name in common.palette():
        mat = bpy.data.materials.get(name)
        if mat is not None:
            mat.name = f"{name} (old)"
    for mat in [m for m in bpy.data.materials if m.name.endswith("(old)")]:
        bpy.data.materials.remove(mat)


def build(detailed=True):
    refresh()
    scene = bpy.data.scenes.get(SCENE) or bpy.data.scenes.new(SCENE)
    for obj in list(scene.objects):
        bpy.data.objects.remove(obj)
    if bpy.context.window is not None:
        bpy.context.window.scene = scene
    skin, eyes, _ = O.fitted(scene)
    # As the model has them: the garments' edges thickened last, a rim only (`models.edges`);
    # thickened first, their inner shells came through the relief.
    G.DEFER_EDGES[0] = True
    try:
        pieces = O.build(scene, skin, eyes)
    finally:
        G.DEFER_EDGES[0] = False
    for obj in (skin, *eyes):
        bpy.data.objects.remove(obj)
    for obj in pieces.pop("base_layer"):
        bpy.data.objects.remove(obj)
    if detailed:
        D.detail(scene, pieces)
    for piece, objs in pieces.items():
        for obj in objs:
            M.edges(obj)
            obj.name = f"{piece}: {obj.name}"
    return pieces


build()
