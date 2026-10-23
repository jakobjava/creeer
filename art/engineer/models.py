"""The engineer as a model (G1.3f; ADR-0026 decisions 6–8): the pieces `outfit.py` builds, in
three levels of detail, divided into regions and skinned on the ragdoll's fifteen segments, in
one file (`data/models/engineer.glb`, made by `art/engineer.py`).

- **Levels** (ADR-0026, Cost: measured before modelling): each piece decimated whole by quadric
  error, so its regions meet without cracks and its triangles stay where it curves. Close-up
  (level 0), mid (1) and far (2): the body and outfit as drawn within 50,000, 15,000 and 3,000
  triangles.
- **Regions** (decision 7, `regions.py`): a wearable hides regions of the body and of the layers
  beneath (`data/wearables.ron`).
- **Skinned** on the ragdoll's segments, named as `creeer_sim::character::bones` names them: each
  bone's head at its joint with its parent (the pelvis's at its centre), in the ragdoll's frame
  (standing on the boots' soles, arms hanging), so a segment's pose moves its vertices. At most
  four bones a vertex, the weights summing to one.

Objects are named `<piece>:<region>@<level>`; the export writes them as the file's nodes.
"""

import math

import bake
import bmesh
import bpy
from mathutils.bvhtree import BVHTree
import detail as D
import inside as I
import outfit as O
import regions as Rg
import skinning as S
import surface as Sf
import trims as T

# Each piece's triangles at the close-up level, all its regions (hidden ones too); the mid and
# far levels take these shares. Worn whole, what is drawn (the body's head and neck, the
# coverall's legs, everything over it) stays within 50,000 / 15,000 / 3,000.
BUDGET = {
    "body": 28_500,
    "base_layer": 5_000,
    "coverall": 9_000,
    "jacket": 9_000,
    "belt": 1_500,
    "gloves": 4_900,
    "boots": 5_700,
    # The pads, their straps and buckles: the straps and buckles whole are 1,180 alone.
    "knee_pads": 1_200,
    "elbow_pads": 700,
    "goggles": 1_200,
}
LEVELS = (1.0, 0.34, 0.065)
# Faces meeting at more than this (degrees) are shaded hard: real edges, not folds.
HARD = 30
# Occlusion rays a corner: creases only (the renderer's ambient occlusion does the rest, G1.3d).
RAYS = 24


def join(scene, name, objs):
    """`objs` (copies) joined into one object `name`."""
    copies = []
    for obj in objs:
        c = obj.copy()
        c.data = obj.data.copy()
        scene.collection.objects.link(c)
        copies.append(c)
    with bpy.context.temp_override(selected_editable_objects=copies, active_object=copies[0], object=copies[0]):
        bpy.ops.object.join()
    out = copies[0]
    out.name = out.data.name = name
    return out


def triangles(obj):
    return sum(len(p.vertices) - 2 for p in obj.data.polygons)


def open_edges(obj):
    """The vertices on `obj`'s open edges (where a garment ends)."""
    uses = {}
    for p in obj.data.polygons:
        for key in p.edge_keys:
            uses[key] = uses.get(key, 0) + 1
    return sorted({i for key, n in uses.items() if n == 1 for i in key})


def decimate(obj, target, spare_ends=True):
    """`obj` collapsed to about `target` triangles by quadric error (Garland and Heckbert 1997,
    Blender's Decimate): triangles stay where the surface curves, the face and the hands, and
    leave where it is flat. Close up (`spare_ends`), its open edges are kept (a group limits the
    collapse to where it weighs; inverted, it spares them): collapsed, a garment's end came out
    ragged. Further off they are not: kept, they held the far level at 10,470 triangles. Sparing the
    face by such a weight was tried: it starved the rest (a torso of 125 triangles, hands of
    none at the mid level)."""
    modifier = obj.modifiers.new("decimate", "DECIMATE")
    rim = open_edges(obj) if spare_ends else []
    modifier.ratio = min(1.0, target / max(1, triangles(obj)))
    modifier.use_collapse_triangulate = True
    if rim:
        group = obj.vertex_groups.new(name="rim")
        group.add(rim, 1.0, "REPLACE")
        modifier.vertex_group, modifier.invert_vertex_group = "rim", True
    with bpy.context.temp_override(object=obj, active_object=obj):
        bpy.ops.object.modifier_apply(modifier="decimate")
    if "rim" in obj.vertex_groups:
        obj.vertex_groups.remove(obj.vertex_groups["rim"])


def clean(obj):
    """`obj`'s faces without area gone: short edges dissolved, and each sliver (three points in a
    line: long edges, no area) its shortest edge collapsed; triangles throughout."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.dissolve_degenerate(bm, dist=1e-5, edges=bm.edges[:])
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 3])
    for _ in range(4):
        slivers = [f for f in bm.faces if f.is_valid and f.calc_area() < 1e-9]
        if not slivers:
            break
        short = {min(f.edges, key=lambda e: e.calc_length()) for f in slivers}
        bmesh.ops.collapse(bm, edges=list(short))
    # Points and edges left without a face (21 points of the head's, in every region's part).
    bmesh.ops.delete(bm, geom=[e for e in bm.edges if not e.link_faces], context="EDGES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bm.to_mesh(obj.data)
    bm.free()


def shaded_hard(e):
    """Whether the edge `e` is shaded hard: marked so, and both its corners split by another
    hard edge (a lone hard edge's corners keep one normal, and it shades smooth)."""
    if e.smooth:
        return False
    return all(sum(1 for x in v.link_edges if not x.smooth) >= 2 for v in e.verts)


def strays(e):
    """How far (m) a smoothly shaded edge's faces stray from the curve they stand for, and how
    far they may (`art-style.md` §7: 1 % of its radius or 1 mm, whichever is more): the rule
    `creeer_art::model_check` holds the close-up level to."""
    if len(e.link_faces) != 2 or shaded_hard(e):
        return 0.0, 1.0
    a, b = e.link_faces
    theta = a.normal.angle(b.normal, 0.0)
    length = e.calc_length()
    if theta < 1e-6 or length <= 0.0:
        return 0.0, 1.0
    chord = min(2.0 * f.calc_area() / length for f in (a, b))
    radius = chord / (2.0 * math.sin(theta / 2.0))
    return chord / 2.0 * math.tan(theta / 4.0), max(0.01 * radius, 0.001)


def refine(obj, source, passes=6):
    """`obj` (decimated from `source`) refined until no smooth edge strays past §7's limit:
    the two faces of each that does split into four, the new corners put back on `source`'s
    surface. Quadric decimation keeps the surface near, not its facets small where it bends:
    left there, faces several centimetres wide bent 20–30° across a fold."""
    tree = BVHTree.FromObject(source, bpy.context.evaluated_depsgraph_get())
    for _ in range(passes):
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        bm.normal_update()
        faces = set()
        for e in bm.edges:
            stray, limit = strays(e)
            if stray > 0.75 * limit:
                faces.update(e.link_faces)
        if not faces:
            bm.free()
            return
        edges_ = list({e for f in faces for e in f.edges})
        made = bmesh.ops.subdivide_edges(bm, edges=edges_, cuts=1, use_grid_fill=True)
        for v in made["geom_inner"] + made["geom_split"]:
            if isinstance(v, bmesh.types.BMVert):
                hit = tree.find_nearest(v.co)[0]
                if hit is not None:
                    v.co = hit
        bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 3])
        bm.to_mesh(obj.data)
        bm.free()
        # Its own slivers collapsed now, so the next pass sees the faces as they will stay.
        clean(obj)


def edges(obj):
    """A garment's ends given their thickness (the `edge` each vertex recorded, `garments.py`):
    a rim turned in from each open edge, no inner shell."""
    thickness = obj.data.attributes.get("edge")
    if thickness is None:
        return
    values = [0.0] * len(obj.data.vertices)
    thickness.data.foreach_get("value", values)
    most = max(values)
    if most <= 0.0:
        return
    group = obj.vertex_groups.new(name="edge")
    for i, t in enumerate(values):
        group.add([i], t / most, "REPLACE")
    solid = obj.modifiers.new("edge", "SOLIDIFY")
    solid.thickness, solid.offset = most, -1.0
    solid.use_rim, solid.use_rim_only = True, True
    solid.vertex_group = "edge"
    with bpy.context.temp_override(object=obj, active_object=obj):
        bpy.ops.object.modifier_apply(modifier="edge")
    obj.vertex_groups.remove(obj.vertex_groups["edge"])
    if "edge" in obj.data.attributes:
        obj.data.attributes.remove(obj.data.attributes["edge"])


def per_vertex(obj):
    """The bake made one value a vertex: its corners' mean colour, and grain coordinates from
    its place (across and up). A smooth character has no hard edges to keep apart, and values
    per corner split its vertices at every face: the file twice the size."""
    mesh = obj.data
    corner = mesh.color_attributes["Color"]
    sums = [[0.0, 0.0, 0.0, 0.0, 0] for _ in mesh.vertices]
    for loop in mesh.loops:
        c, s = corner.data[loop.index].color, sums[loop.vertex_index]
        for i in range(4):
            s[i] += c[i]
        s[4] += 1
    mesh.color_attributes.remove(corner)
    point = mesh.color_attributes.new("Color", "FLOAT_COLOR", "POINT")
    for v, s in zip(point.data, sums):
        n = max(1, s[4])
        v.color = (s[0] / n, s[1] / n, s[2] / n, s[3] / n)
    mesh.color_attributes.active_color = point
    uv = mesh.uv_layers["UVMap"]
    for loop in mesh.loops:
        co = mesh.vertices[loop.vertex_index].co
        uv.data[loop.index].uv = (co.x - co.y, co.z)


def copy(scene, obj):
    out = obj.copy()
    out.data = obj.data.copy()
    scene.collection.objects.link(out)
    return out


def level(scene, joined, hardware, piece, k, rig, covers):
    """Piece `joined` at level `k`: decimated, its edges thickened, its small hardware (a
    `hardware` object per level, or None) added whole, baked from the ground up, triangulated,
    divided into its regions (what the wearables over it enclose a region of its own,
    `inside.Covers`). Decimated with the cloth, a button or an eyelet went first (its error is
    small for being small) and came out a blob; whole, it is drawn close up, and the larger
    pieces of it mid way too (`trims.place`)."""
    obj = copy(scene, joined)
    extra = copy(scene, hardware[k]) if hardware and hardware[k] is not None else None
    budget = BUDGET[piece] * LEVELS[k]
    decimate(obj, max(budget - (triangles(extra) if extra else 0), 0.5 * budget), spare_ends=k == 0)
    # Cleaned before refining: collapsed afterwards, a sliver reshaped the faces beside it.
    clean(obj)
    edges(obj)
    # Real edges shaded hard again (a panel's step, a garment's cut edge): decimation leaves
    # its new edges smooth, and smooth across a right angle a face strays from any curve. Then
    # refined, judging each edge as it will shade.
    T.sharpen(obj.data, HARD, keep=True)
    if k == 0:
        refine(obj, joined)
    if piece == "body":
        Rg.cut_openings(obj, O.openings())
    if piece == "boots":
        # Weighted again once cut: a new point takes its edge's weights in proportion, and a
        # long edge across the bend gave its top 0.6 of the foot's.
        Rg.cut(obj, S.bends())
        S.stiff(obj)
    if extra is not None:
        with bpy.context.temp_override(selected_editable_objects=[obj, extra], active_object=obj, object=obj):
            bpy.ops.object.join()
    bake.bake(obj, rays=RAYS, base=0.0)
    per_vertex(obj)
    Sf.wear(obj, piece)
    Sf.stains(obj, piece)
    Sf.hair_grain(obj)
    triangulate = obj.modifiers.new("triangles", "TRIANGULATE")
    triangulate.quad_method, triangulate.ngon_method = "FIXED", "CLIP"
    triangulate.keep_custom_normals = True
    with bpy.context.temp_override(object=obj, active_object=obj):
        bpy.ops.object.modifier_apply(modifier="triangles")
    clean(obj)
    regions = covers.relabel(obj, piece, k, Rg.face_regions(obj, piece, O.openings()))
    parts = Rg.split(scene, obj, piece, k, regions, rig)
    bpy.data.objects.remove(obj)
    return parts


def build():
    """The engineer, every piece at every level, in the current scene; returns its parts."""
    scene = bpy.context.scene
    skin, eyes, report = O.fitted(scene)
    for name, value in report.items():
        print(f"art: {name}: {value}")
    O.G.DEFER_EDGES[0] = True
    try:
        pieces = O.build(scene, skin, eyes)
    finally:
        O.G.DEFER_EDGES[0] = False
    for obj in (skin, *eyes):
        bpy.data.objects.remove(obj)
    D.detail(scene, pieces)
    S.complete_weights(pieces, next(o for o in pieces["body"] if o.name == "skin"))
    S.measure_fingers(next(o for o in pieces["body"] if o.name == "skin"), O.SOLE)
    covers = I.Covers(next(o for o in pieces["body"] if o.name == "skin"))
    rig = S.armature(scene)
    parts = []
    for piece in I.order(pieces):
        objs = pieces[piece]
        # Hardware: whole at the close-up level; then a "mid" piece whole mid way too and gone
        # far off, a "whole" one (a strap) decimated with the cloth, a "close" one gone.
        Sf.number_parts(objs)
        cloth = [o for o in objs if not o.get("hardware")]
        whole = [o for o in objs if o.get("hardware") == "whole"]
        small = [o for o in objs if o.get("hardware")]
        mid = [o for o in small if o["hardware"] == "mid"]
        joined = (join(scene, piece, cloth), join(scene, piece, cloth + whole) if whole else None)
        hardware = (join(scene, piece, small) if small else None, join(scene, piece, mid) if mid else None, None)
        for k in range(len(LEVELS)):
            made = level(scene, joined[1] if k and joined[1] else joined[0], hardware, piece, k, rig, covers)
            covers.add(piece, k, made)
            parts += made
        for obj in (*joined, *hardware):
            if obj is not None:
                bpy.data.objects.remove(obj)
    for objs in pieces.values():
        for obj in objs:
            bpy.data.objects.remove(obj)
    for k in range(len(LEVELS)):
        drawn = sum(triangles(p) for p in parts if p.name.endswith(f"@{k}"))
        print(f"art: level {k}: {drawn} triangles in all its pieces and regions")
    return parts
