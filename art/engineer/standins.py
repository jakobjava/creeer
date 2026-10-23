"""Stand-ins for the engineer's levels of detail (G1.3f; ADR-0026, Cost): the blockout's outfit
figure merged and decimated to each level's triangle count, written to `target/bench/` for
`creeer_client --bench 600 --view factory --players target/bench/engineer_15k.glb 16`. Run in the
live session after `blockout.py` has built its scene:

    exec(open("art/engineer/standins.py").read())

All in one material (`coverall`): what is measured is the triangles' cost, not the look.
"""

import os
import pathlib

import bpy

try:
    ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
except NameError:  # run by exec() in a live session
    ROOT = pathlib.Path(r"C:\Users\jakmo\dev\creëer")
OUT = ROOT / "target" / "bench"
LEVELS = (80_000, 40_000, 15_000, 3_000)


def triangles(mesh):
    return sum(len(p.vertices) - 2 for p in mesh.polygons)


def export(source="engineer blockout"):
    src = bpy.data.scenes[source]
    outfit = [o for o in src.objects if o.name.startswith("outfit ") and o.type == "MESH"]
    material = bpy.data.materials.get("coverall") or bpy.data.materials.new("coverall")
    scene = bpy.data.scenes.new("stand-in export")
    shown = bpy.context.window.scene
    bpy.context.window.scene = scene
    os.makedirs(OUT, exist_ok=True)
    try:
        for level in LEVELS:
            for o in list(scene.objects):
                bpy.data.objects.remove(o)
            copies = []
            for o in outfit:
                c = o.copy()
                c.data = o.data.copy()
                c.location = (0.0, 0.0, 0.0)
                scene.collection.objects.link(c)
                copies.append(c)
            with bpy.context.temp_override(selected_editable_objects=copies, active_object=copies[0], object=copies[0]):
                bpy.ops.object.join()
            merged = copies[0]
            decimate = merged.modifiers.new("decimate", "DECIMATE")
            decimate.ratio = min(1.0, level / triangles(merged.data))
            with bpy.context.temp_override(object=merged, active_object=merged):
                bpy.ops.object.modifier_apply(modifier="decimate")
            merged.data.materials.clear()
            merged.data.materials.append(material)
            for p in merged.data.polygons:
                p.material_index = 0
            for name in list(merged.data.color_attributes.keys()):
                merged.data.color_attributes.remove(merged.data.color_attributes[name])
            path = OUT / f"engineer_{level // 1000}k.glb"
            bpy.ops.export_scene.gltf(filepath=str(path), use_active_scene=True, export_format="GLB", export_yup=True, export_apply=True)
            print(f"art: {path.name}: {triangles(merged.data)} triangles")
        for o in list(scene.objects):
            bpy.data.objects.remove(o)
    finally:
        bpy.context.window.scene = shown
        bpy.data.scenes.remove(scene)


export()
