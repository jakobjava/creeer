"""Shared helpers for Creëer's model scripts (docs/blender-mcp.md, ADR-0026).

Every model script in art/ defines build(), which makes its model from an empty scene with
nothing but these modules. The same script runs live in the user's Blender through the MCP while
it is made, and headless (art/build.py, `cargo xtask art`) when the .glb is exported. It must be
deterministic: no randomness without a fixed seed, no dependence on what was in the scene.

Conventions (docs/art-style.md is the authority):
- metres, Blender's +Z up (the exporter turns it into glTF's +Y up);
- one material per palette token, named exactly after it; its look in Blender is a preview
  (preview.py) of the game's surfaces;
- every part made with box() is tagged with its part id and grain axis, which bake.py turns
  into the "Color" attribute and the "UVMap" the renderer's surfaces use.
"""

import pathlib
import re

import bmesh
import bpy
from mathutils import Vector

import bake
import preview

ROOT = pathlib.Path(__file__).resolve().parent.parent
PALETTE = ROOT / "data" / "art" / "palette.ron"

# Edges whose faces meet at more than this are sharp (shaded hard), in radians.
SHARP = bake.SHARP
# The last part id given out (reset() starts again, so ids are the same every build).
_last_part = [0]


def reset():
    """Removes every mesh object, mesh and material; cameras and lights stay (not exported)."""
    for obj in list(bpy.data.objects):
        if obj.type not in {"CAMERA", "LIGHT"}:
            bpy.data.objects.remove(obj)
    for mesh in list(bpy.data.meshes):
        bpy.data.meshes.remove(mesh)
    for material in list(bpy.data.materials):
        bpy.data.materials.remove(material)
    units = bpy.context.scene.unit_settings
    units.system = "METRIC"
    units.scale_length = 1.0
    _last_part[0] = 0


def _srgb_to_linear(c):
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def palette():
    """Token -> (linear rgb, roughness, metalness, pattern, wears_to, ages_to, weathers_in) from
    data/art/palette.ron, for previewing."""
    text = PALETTE.read_text(encoding="utf-8")
    tokens = {}
    for line in text.splitlines():
        m = re.search(r'\(name: "(\w+)",.*?colour: (?:Hex\("(#[0-9a-f]{6})"\)|\w+)(.*)', line)
        if not m:
            continue
        name, hexcode, rest = m.group(1), m.group(2), m.group(3)

        def field(pattern, default, rest=rest):
            found = re.search(pattern, rest)
            return found.group(1) if found else default

        rgb = (0.5, 0.5, 0.5)
        if hexcode:
            rgb = tuple(_srgb_to_linear(int(hexcode[i : i + 2], 16) / 255) for i in (1, 3, 5))
        tokens[name] = (
            rgb,
            float(field(r"roughness: ([0-9.]+)", 1.0)),
            float(field(r"metalness: ([0-9.]+)", 0.0)),
            field(r"pattern: (\w+)", "Plain"),
            field(r'wears_to: Some\("(\w+)"\)', None),
            field(r'ages_to: Some\("(\w+)"\)', None),
            float(years) if (years := field(r"weathers_in: Some\(([0-9.]+)\)", None)) else None,
        )
    return tokens


def material(token):
    """The material named `token` (created once), with the preview of its surface."""
    existing = bpy.data.materials.get(token)
    if existing:
        return existing
    tokens = palette()
    if token not in tokens:
        raise ValueError(f"{token} is not a palette token")
    rgb, rough, metal, pattern, wears_to, ages_to, weathers_in = tokens[token]
    mat = bpy.data.materials.new(token)
    mat.use_nodes = True
    mat.use_backface_culling = True
    wears = tokens[wears_to][:3] if wears_to else None
    ages = tokens[ages_to][0] if ages_to else None
    preview.surface(mat, (rgb, rough, metal, pattern, weathers_in), wears, ages)
    return mat


def box(bm, lo, hi, grain=None):
    """Adds an axis-aligned box from corner `lo` to `hi` to `bm` as a new part whose grain runs
    along axis `grain` (its longest by default); returns its faces."""
    # The layers first: adding a layer invalidates the faces already taken from `bm`.
    part = bm.faces.layers.int.get("part") or bm.faces.layers.int.new("part")
    axis = bm.faces.layers.int.get("grain") or bm.faces.layers.int.new("grain")
    lo, hi = Vector(lo), Vector(hi)
    size, centre = hi - lo, (hi + lo) / 2
    made = bmesh.ops.create_cube(bm, size=1.0)
    verts = made["verts"]
    for v in verts:
        v.co = Vector((v.co.x * size.x, v.co.y * size.y, v.co.z * size.z)) + centre
    # In creation order: a set would iterate in memory order and make the build differ per run.
    faces = list(dict.fromkeys(f for v in verts for f in v.link_faces))
    _last_part[0] += 1
    along = grain if grain is not None else max(range(3), key=lambda a: size[a])
    for f in faces:
        f[part] = _last_part[0]
        f[axis] = along
    return faces


def bevel_edges(bm, edges, width, segments=1):
    """Bevels `edges` of `bm` (a chamfer when `segments` is 1); the new faces keep their part."""
    bmesh.ops.bevel(
        bm,
        geom=list(edges),
        offset=width,
        offset_type="OFFSET",
        segments=segments,
        profile=0.5,
        affect="EDGES",
        clamp_overlap=True,
    )


def part(name, bm, token):
    """An object called `name` made of `bm` in the material `token`."""
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(material(token))
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def join(name, parts):
    """Joins `parts` into one object `name`, smooth-shaded with face-weighted normals."""
    for obj in bpy.context.selected_objects:
        obj.select_set(False)
    for obj in parts:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    obj.name = name
    obj.data.name = name
    obj.data.shade_smooth()
    modifier = obj.modifiers.new("weighted normals", "WEIGHTED_NORMAL")
    modifier.mode = "FACE_AREA"
    modifier.weight = 100
    modifier.keep_sharp = True
    modifier.thresh = 0.01
    # Sharp edges: every edge whose faces meet at more than 30 degrees stays crisp.
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    sharp = bm.edges.layers.bool.get("sharp_edge") or bm.edges.layers.bool.new("sharp_edge")
    for edge in bm.edges:
        edge[sharp] = len(edge.link_faces) != 2 or edge.calc_face_angle() > SHARP
    bm.to_mesh(obj.data)
    bm.free()
    return obj


def apply_modifiers(obj):
    """Replaces `obj`'s mesh by its evaluated one (modifiers applied, custom normals kept)."""
    depsgraph = bpy.context.evaluated_depsgraph_get()
    evaluated = obj.evaluated_get(depsgraph)
    mesh = bpy.data.meshes.new_from_object(evaluated, preserve_all_data_layers=True, depsgraph=depsgraph)
    old = obj.data
    obj.modifiers.clear()
    obj.data = mesh
    bpy.data.meshes.remove(old)
    mesh.name = obj.name


def finish(obj):
    """Applies the modifiers, bakes what the renderer's surfaces need (bake.py) and triangulates:
    the model as it is exported."""
    apply_modifiers(obj)
    bake.bake(obj)
    # Triangulated here by fixed rules, keeping the normals and the bake: left to the exporter,
    # quads and bevelled n-gons came out triangulated differently from one build to the next.
    triangulate = obj.modifiers.new("triangles", "TRIANGULATE")
    triangulate.quad_method = "FIXED"
    triangulate.ngon_method = "CLIP"
    triangulate.keep_custom_normals = True
    apply_modifiers(obj)
    return obj


def triangles(obj):
    """The model's triangle count."""
    return sum(len(p.vertices) - 2 for p in obj.data.polygons)


def export(path):
    """Writes every mesh in the scene to `path` as a binary glTF, with fixed options; skinned on
    the scene's armature if it has one (the engineer)."""
    bpy.ops.export_scene.gltf(
        filepath=str(path),
        export_format="GLB",
        use_selection=False,
        export_apply=False,
        export_yup=True,
        export_texcoords=True,
        export_normals=True,
        export_tangents=False,
        export_materials="EXPORT",
        export_image_format="NONE",
        export_vertex_color="NAME",
        export_vertex_color_name="Color",
        export_all_vertex_colors=False,
        export_active_vertex_color_when_no_material=False,
        export_animations=False,
        export_skins=any(o.type == "ARMATURE" for o in bpy.context.scene.objects),
        export_morph=False,
        export_extras=False,
        export_cameras=False,
        export_lights=False,
    )
