"""The engineer as the game draws it, live (G1.3f): one level of the exported model
(`cargo xtask art --blend engineer` writes `target/art/engineer.blend`) in the game's surfaces
(`preview.py`: each token's pattern, the parts' tones, mottling, grime, wear, the baked
occlusion), the players' tokens in player one's colours, aged as a worn outfit is, under the
game's sun and sky on soil as the style board stages the crate (`board.py`). The full outfit is
worn: what it hides (`data/wearables.ron`) is left out, as the game leaves it. Run it in a
Blender session (it builds into its own scene, "engineer level <k>"):

    exec(open("art/engineer/review_model.py").read())
"""

import pathlib
import sys

import bpy

try:
    HERE = pathlib.Path(__file__).resolve().parent
except NameError:  # run by exec() in a live session
    HERE = pathlib.Path(r"C:\Users\jakmo\dev\creëer\art\engineer")
ROOT = HERE.parent.parent
for path in (HERE, HERE.parent):
    if str(path) not in sys.path:
        sys.path.insert(0, str(path))
import importlib  # noqa: E402

import board  # noqa: E402
import common  # noqa: E402
import outfit as O  # noqa: E402
import preview  # noqa: E402
import wearables as W  # noqa: E402

preview = importlib.reload(preview)

LEVEL = 0
# Years the outfit has been worn: grime gathers over two (preview.GRIME_YEARS); a work suit
# worn three is dirty through, its edges chipped.
AGE = 3.0


def material(token):
    """The preview of `token`'s surface, the per-player ones in player one's colours."""
    name = f"surface {token}"
    if name in bpy.data.materials:
        return bpy.data.materials[name]
    tokens = common.palette()
    rgb, rough, metal, pattern, wears_to, ages_to, weathers_in = tokens[token]
    if token in O.PREVIEW:
        rgb = O.preview_colour(token)
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    wears = tokens[wears_to][:3] if wears_to else None
    ages = tokens[ages_to][0] if ages_to else None
    preview.surface(mat, (rgb, rough, metal, pattern, weathers_in), wears, ages)
    return mat


def stage(scene):
    """The style board's ground, sun and sky (`board.py`), and its renderer and view."""
    if "soil" not in scene.objects:
        board.ground()
        bpy.context.active_object.name = "soil"
    if "sun" not in scene.objects:
        board.camera_and_sun()
    engines = [e.identifier for e in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items]
    scene.render.engine = next((e for e in engines if e.startswith("BLENDER_EEVEE")), engines[0])
    transforms = [t.identifier for t in bpy.types.ColorManagedViewSettings.bl_rna.properties["view_transform"].enum_items]
    if "AgX" in transforms:
        scene.view_settings.view_transform = "AgX"


def shown(name, hidden):
    piece, rest = name.split(":")
    return rest.split("@")[0] not in hidden.get(piece, ())


def build(level=LEVEL):
    scene_name = f"engineer level {level}"
    scene = bpy.data.scenes.get(scene_name) or bpy.data.scenes.new(scene_name)
    for obj in list(scene.objects):
        if obj.name not in ("soil", "sun", "camera"):
            bpy.data.objects.remove(obj)
    if bpy.context.window is not None:
        bpy.context.window.scene = scene
    stage(scene)
    path = str(ROOT / "target" / "art" / "engineer.blend")
    table = W.hides()
    hidden = W.hidden(table, table, ["body", *table])
    with bpy.data.libraries.load(path, link=False) as (source, target):
        target.objects = [n for n in source.objects if n.endswith(f"@{level}") and shown(n, hidden)]
    for obj in target.objects:
        scene.collection.objects.link(obj)
        obj.parent = None
        for mod in list(obj.modifiers):
            obj.modifiers.remove(mod)
        # The skin and hair washed: they take no weathering's streaks.
        obj["age"] = 0.3 if obj.name.startswith("body:") else AGE
        for slot in obj.material_slots:
            token = slot.material.name.split(".")[0]
            slot.material = material(token)
    return scene


if __name__ != "review_model":  # run by exec() in a session; imported, it waits (render_review.py)
    build()
