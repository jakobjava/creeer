"""Renders of the engineer as the game draws it (G1.3f), headless: the exported model's level
(`review_model.py`: the game's surfaces, aged, on the style board's soil under its sun and sky)
seen from several sides at play distance and close up, written as PNGs. Run

    blender --background --factory-startup --python art/engineer/render_review.py -- <out dir> [<level>]

after `cargo xtask art --blend engineer`.
"""

import math
import pathlib
import sys

import bpy
from mathutils import Vector

HERE = pathlib.Path(__file__).resolve().parent
for path in (HERE, HERE.parent):
    sys.path.insert(0, str(path))

import review_model  # noqa: E402  (after the path is set)

# The views: (name, the eye's direction from the target, the target (m, up), distance (m),
# the lens's vertical field of view (degrees)).
VIEWS = (
    ("front", (0.35, -1.0, 0.25), 0.95, 3.4, 40.0),
    ("back", (-0.4, 1.0, 0.3), 0.95, 3.4, 40.0),
    ("side", (1.0, -0.15, 0.15), 0.95, 3.4, 40.0),
    ("above", (0.4, -0.7, 1.0), 0.9, 4.2, 40.0),
    ("chest", (0.3, -1.0, 0.1), 1.3, 1.0, 40.0),
    ("legs", (0.5, -1.0, 0.25), 0.45, 1.3, 40.0),
    ("head", (0.45, -1.0, 0.1), 1.68, 0.55, 40.0),
)


def render(out, level):
    scene = review_model.build(level)
    scene.render.resolution_x, scene.render.resolution_y = 900, 1200
    scene.render.image_settings.file_format = "PNG"
    data = bpy.data.cameras.new("review")
    data.sensor_fit = "VERTICAL"
    cam = bpy.data.objects.new("review", data)
    scene.collection.objects.link(cam)
    scene.camera = cam
    for name, way, height, distance, fov in VIEWS:
        data.angle_y = math.radians(fov)
        target = Vector((0.0, 0.0, height))
        cam.location = target + Vector(way).normalized() * distance
        cam.rotation_euler = (target - cam.location).to_track_quat("-Z", "Y").to_euler()
        scene.render.filepath = str(out / f"engineer-{level}-{name}.png")
        bpy.ops.render.render(write_still=True)
        print(f"art: render -> {scene.render.filepath}")


def main():
    args = sys.argv[sys.argv.index("--") + 1 :]
    out = pathlib.Path(args[0]).resolve()
    out.mkdir(parents=True, exist_ok=True)
    render(out, int(args[1]) if len(args) > 1 else 0)


try:
    main()
except Exception:  # Blender would otherwise keep running with exit code 0
    import traceback

    traceback.print_exc()
    sys.exit(1)
