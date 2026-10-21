"""The style board in Blender (ADR-0026, G1.3c): the crate new, weathered a year and four years,
on soil, as `creeer_client --style-board` draws it, so the preview (preview.py) and the game's
shader can be judged side by side. `cargo xtask art --board <out.png>` runs

    blender --background --factory-startup --python art/board.py -- <out.png>

The scene follows the game's: its camera, field of view and sun; glTF's axes (the game's Y up
is Blender's Z up, its −Z Blender's +Y). Lighting and tone mapping are Blender's own, so the
two images agree in layers, patterns and how age shows, not to the last level.
"""

import math
import pathlib
import sys

import bpy
from mathutils import Vector

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import common  # noqa: E402  (after the path is set)
import crate  # noqa: E402

# The crates' weathering (years), as style_board.rs.
AGES = (0.0, 1.0, 4.0)
# Soil (art-style.md §4), sRGB.
SOIL = (0x6B, 0x4F, 0x35)


def game_to_blender(x, y, z):
    return Vector((x, -z, y))


def ground():
    """The slab, 12 m by 8 m, its top at the crates' base."""
    bpy.ops.mesh.primitive_cube_add(size=1.0, location=(0.0, 0.0, -0.1))
    slab = bpy.context.active_object
    slab.scale = (12.0, 8.0, 0.2)
    mat = bpy.data.materials.new("soil")
    mat.use_nodes = True
    bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
    linear = tuple(common._srgb_to_linear(c / 255) for c in SOIL)
    bsdf.inputs["Base Color"].default_value = (*linear, 1.0)
    bsdf.inputs["Roughness"].default_value = 0.92
    slab.data.materials.append(mat)


def crates():
    """The crate three times, a quarter turn apart, each with its years in "age"."""
    model = crate.build()
    for k, years in enumerate(AGES):
        obj = model if k == 0 else model.copy()
        if k > 0:
            bpy.context.scene.collection.objects.link(obj)
        obj.location = game_to_blender((k - 1) * 1.6, 0.0, 0.0)
        # A turn about the game's Y is one about Blender's Z, the same way.
        obj.rotation_euler = (0.0, 0.0, 0.35 * k)
        obj["age"] = years


def camera_and_sun():
    scene = bpy.context.scene
    data = bpy.data.cameras.new("camera")
    data.sensor_fit = "VERTICAL"
    data.angle_y = math.radians(70.0)
    cam = bpy.data.objects.new("camera", data)
    scene.collection.objects.link(cam)
    cam.location = game_to_blender(0.0, 1.55, 3.1)
    # Looking along the game's −Z (Blender's +Y), pitched down 0.3 rad.
    cam.rotation_euler = (math.pi / 2 - 0.3, 0.0, 0.0)
    scene.camera = cam
    # The sun at 50° elevation, 35° azimuth (demo::sun_direction), shining towards the scene.
    e, a = math.radians(50.0), math.radians(35.0)
    towards = game_to_blender(math.cos(e) * math.cos(a), math.sin(e), math.cos(e) * math.sin(a))
    light = bpy.data.lights.new("sun", "SUN")
    light.energy = 3.4
    light.color = (1.0, 0.94, 0.86)
    light.angle = math.radians(0.53)
    sun = bpy.data.objects.new("sun", light)
    scene.collection.objects.link(sun)
    sun.rotation_euler = towards.to_track_quat("Z", "Y").to_euler()
    # A sky of the game's ambient colour.
    world = bpy.data.worlds.new("sky")
    world.use_nodes = True
    background = next(n for n in world.node_tree.nodes if n.type == "BACKGROUND")
    background.inputs["Color"].default_value = (0.24, 0.38, 0.62, 1.0)
    background.inputs["Strength"].default_value = 0.6
    scene.world = world


def render(out):
    scene = bpy.context.scene
    engines = [e.identifier for e in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items]
    scene.render.engine = next((e for e in engines if e.startswith("BLENDER_EEVEE")), engines[0])
    transforms = [t.identifier for t in bpy.types.ColorManagedViewSettings.bl_rna.properties["view_transform"].enum_items]
    if "AgX" in transforms:
        scene.view_settings.view_transform = "AgX"
    scene.render.resolution_x, scene.render.resolution_y = 1600, 900
    scene.render.filepath = str(out)
    scene.render.image_settings.file_format = "PNG"
    bpy.ops.render.render(write_still=True)


def main():
    out = pathlib.Path(sys.argv[sys.argv.index("--") + 1]).resolve()
    common.reset()
    ground()
    crates()
    camera_and_sun()
    out.parent.mkdir(parents=True, exist_ok=True)
    render(out)
    print(f"art: board -> {out}")


try:
    main()
except Exception:  # Blender would otherwise keep running with exit code 0
    import traceback

    traceback.print_exc()
    sys.exit(1)
