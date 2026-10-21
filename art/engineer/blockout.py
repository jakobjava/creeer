"""The engineer's blockout (G1.3f; ADR-0026 decision 8; docs/art-style.md §8): rough shapes of a
real person, to judge the silhouette before any detailed model. Two figures side by side: the
base body in its base layer, and the starting outfit (quilted coverall, jacket in the player's
colour with a belt, leather knee and elbow pads, thick gloves, laced boots, a padded hood
leaving the face open, and goggles). The jetpack is not part of it: it is made as the player
progresses. The rough guide is Factorio's FFF-40 engineer, without its hard hat, tools and
rucksack.

The person: ANSUR II's men (4,082 measured, 2010–2012; the U.S. Army's public data) at their
means, scaled to a stature of 1.80 m (their mean is 1.756 m), standing on boots' 3 cm soles.
The ragdoll's segments (bones.json, written from the Rust code) are drawn beside each figure as
wire boxes: where they disagree with the person, the segments are to be corrected (G1.3f,
"within its segments").

Not a model: nothing is exported. Run it in a Blender session (it builds into its own scene,
"engineer blockout", and leaves the others alone):

    exec(open("art/engineer/blockout.py").read())

Skin, hair and the base layer's colours are placeholders until the user approves the tones.
"""

import json
import math
import pathlib

import bmesh
import bpy
from mathutils import Matrix, Quaternion, Vector

try:
    HERE = pathlib.Path(__file__).resolve().parent
except NameError:  # run by exec() in a live session
    HERE = pathlib.Path(r"C:\Users\jakmo\dev\creëer\art\engineer")
BONES = json.loads((HERE / "bones.json").read_text(encoding="utf-8"))

SCENE = "engineer blockout"
# Placeholders (skin: the Monk scale's sixth tone) and the palette's tokens, sRGB.
COLOURS = {
    "skin": ("#a07e56", 0.55),
    "hair": ("#3b2a20", 0.70),
    "base_layer": ("#5a6066", 0.85),
    "coverall": ("#8c99a6", 0.85),
    "jacket": ("#e07b39", 0.75),
    "leather": ("#4b2c22", 0.70),
    "lens": ("#2a3b33", 0.05),
    "steel": ("#8a8d90", 0.45),
    "rubber": ("#2b2a29", 0.85),
}

# ANSUR II men's means scaled to 1.80 m (m): heights above the soles of bare feet.
TOP = 1.80
CHIN = 1.555  # top − (sellion to menton 0.126 + sellion below the top ~0.12)
EYES = 1.67
NECK_BASE = 1.475  # suprasternale; cervicale (C7) 1.555
SHOULDER = 1.43  # the shoulder joints, 4–5 cm below the acromion (1.477)
ELBOW = 1.133  # radiale: acromion − acromion-radiale length 0.344
WRIST = 0.869
FINGERTIPS = 0.671  # wrist − hand length 0.198
HIP = 0.92  # the hip joints, about at trochanterion (0.923)
KNEE = 0.49  # between the femoral epicondyle (0.504) and the tibial plateau (0.480)
ANKLE = 0.075  # lateral malleolus
SOLE = 0.03  # boots' soles
# The trunk's sections: height, half width, half depth, the centre's offset forward.
TRUNK = [
    (0.84, 0.15, 0.1, 0.0),
    (0.9, 0.172, 0.122, -0.012),
    (0.97, 0.177, 0.126, -0.012),
    (1.04, 0.168, 0.12, -0.006),
    (1.1, 0.162, 0.118, 0.0),
    (1.18, 0.158, 0.121, 0.004),
    (1.26, 0.16, 0.128, 0.006),
    (1.33, 0.164, 0.13, 0.004),
    (1.39, 0.168, 0.124, -0.004),
    (1.43, 0.165, 0.11, -0.01),
    (1.465, 0.145, 0.09, -0.014),
    (1.5, 0.105, 0.072, -0.016),
    (1.53, 0.068, 0.062, -0.014),
]


def g(x, y, z):
    """The game's axes (y up, z forward) in Blender's (z up, the figure facing −y)."""
    return Vector((x, -z, y))


def srgb(hexcode):
    c = [int(hexcode[i : i + 2], 16) / 255 for i in (1, 3, 5)]
    return tuple(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c)


def material(name):
    mat = bpy.data.materials.get("blockout " + name)
    if mat is None:
        mat = bpy.data.materials.new("blockout " + name)
        mat.use_nodes = True
        bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
        colour, rough = COLOURS[name]
        bsdf.inputs["Base Color"].default_value = (*srgb(colour), 1.0)
        bsdf.inputs["Roughness"].default_value = rough
        if name == "steel":
            bsdf.inputs["Metallic"].default_value = 1.0
        mat.diffuse_color = (*srgb(colour), 1.0)
    return mat


class Figure:
    """One figure's parts, gathered by material, standing at `offset` (m, across) and lifted by
    `lift` (m)."""

    def __init__(self, scene, offset, lift=0.0):
        self.scene, self.offset, self.lift, self.parts = scene, offset, lift, {}

    def _add(self, mat, obj):
        self.parts.setdefault(mat, []).append(obj)
        return obj

    def _object(self, bm, name, matrix):
        """A mesh object of `bm` with `matrix` baked into its vertices (the object stays at
        the identity, so sizes in its mesh are metres)."""
        bmesh.ops.transform(bm, matrix=matrix, verts=bm.verts)
        mesh = bpy.data.meshes.new(name)
        bm.to_mesh(mesh)
        bm.free()
        obj = bpy.data.objects.new(name, mesh)
        self.scene.collection.objects.link(obj)
        return obj

    def at(self, p):
        return g(p[0], p[1] + self.lift, p[2]) + Vector((self.offset, 0, 0))

    def _place(self, centre, turn=None, scale=(1.0, 1.0, 1.0)):
        at = Matrix.Translation(self.at(centre))
        rotation = (turn or Quaternion()).to_matrix().to_4x4()
        return at @ rotation @ Matrix.Diagonal((*scale, 1.0))

    def ellipsoid(self, mat, centre, semi):
        """An ellipsoid at `centre` with semi-axes `semi`, both in the game's axes."""
        bm = bmesh.new()
        bmesh.ops.create_uvsphere(bm, u_segments=32, v_segments=16, radius=1.0)
        matrix = self._place(centre, scale=(semi[0], semi[2], semi[1]))
        return self._add(mat, self._object(bm, mat, matrix))

    def limb(self, mat, a, b, ra, rb):
        """A tapered limb from `a` (radius `ra`) to `b` (radius `rb`), rounded at both ends."""
        axis = g(*b) - g(*a)
        bm = bmesh.new()
        bmesh.ops.create_cone(
            bm, cap_ends=True, segments=32, radius1=ra, radius2=rb, depth=axis.length
        )
        mid = [(p + q) / 2 for p, q in zip(a, b)]
        turn = Vector((0, 0, 1)).rotation_difference(axis.normalized())
        obj = self._add(mat, self._object(bm, mat, self._place(mid, turn)))
        self.ellipsoid(mat, a, (ra, ra, ra))
        self.ellipsoid(mat, b, (rb, rb, rb))
        return obj

    def loft(self, mat, rings, segments=40):
        """A smooth trunk through elliptical `rings` (height, half width, half depth, the
        centre's offset forward; game axes), closed at both ends."""
        bm = bmesh.new()
        loops = []
        for y, hw, hd, zc in rings:
            loops.append([
                bm.verts.new(g(hw * math.cos(a), y, zc + hd * math.sin(a)))
                for a in (math.tau * i / segments for i in range(segments))
            ])
        for lo, hi in zip(loops, loops[1:]):
            for i in range(segments):
                j = (i + 1) % segments
                bm.faces.new((lo[i], lo[j], hi[j], hi[i]))
        for ring, flip in ((loops[0], True), (loops[-1], False)):
            bm.faces.new(ring[::-1] if flip else ring)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        offset = Matrix.Translation(self.at((0, 0, 0)))
        return self._add(mat, self._object(bm, mat, offset))

    def disc(self, mat, centre, radius, depth, facing=(0, 0, 1)):
        """A short cylinder (a lens, a cap) at `centre`, its axis along `facing` (game axes)."""
        bm = bmesh.new()
        bmesh.ops.create_cone(
            bm, cap_ends=True, segments=24, radius1=radius, radius2=radius, depth=depth
        )
        turn = Vector((0, 0, 1)).rotation_difference(g(*facing))
        return self._add(mat, self._object(bm, mat, self._place(centre, turn)))

    def tube(self, mat, points, radius):
        """A strap or rail along `points` (game axes)."""
        curve = bpy.data.curves.new(mat, "CURVE")
        curve.dimensions = "3D"
        curve.bevel_depth = radius
        curve.bevel_resolution = 3
        spline = curve.splines.new("POLY")
        spline.points.add(len(points) - 1)
        for p, q in zip(spline.points, points):
            p.co = (*self.at(q), 1.0)
        obj = bpy.data.objects.new(mat, curve)
        self.scene.collection.objects.link(obj)
        return self._add(mat, obj)

    def finish(self, name, soft):
        """Joins each material's parts into one object; the `soft` ones (cloth and skin) are
        merged into one smooth surface by a voxel remesh and smoothing."""
        out = []
        for mat, objs in self.parts.items():
            with bpy.context.temp_override(selected_editable_objects=objs, active_object=objs[0], object=objs[0]):
                bpy.ops.object.convert(target="MESH")
            names = {x.name for x in objs}
            meshes = [o for o in self.scene.objects if o.name in names]
            if len(meshes) > 1:
                with bpy.context.temp_override(selected_editable_objects=meshes, active_object=meshes[0], object=meshes[0]):
                    bpy.ops.object.join()
            obj = meshes[0]
            obj.name = f"{name} {mat}"
            obj.data.materials.clear()
            obj.data.materials.append(material(mat))
            if mat in soft:
                remesh = obj.modifiers.new("merge", "REMESH")
                remesh.mode = "VOXEL"
                remesh.voxel_size = 0.01
                smooth = obj.modifiers.new("smooth", "SMOOTH")
                smooth.factor = 0.9
                smooth.iterations = 20
                with bpy.context.temp_override(object=obj, active_object=obj):
                    bpy.ops.object.modifier_apply(modifier="merge")
                    bpy.ops.object.modifier_apply(modifier="smooth")
            for p in obj.data.polygons:
                p.use_smooth = True
            out.append(obj)
        return out


def grown(lo, hi, more):
    """The trunk's sections from `lo` to `hi` (m), grown by `more` (a garment's thickness)."""
    return [(y, hw + more, hd + more, zc) for y, hw, hd, zc in TRUNK if lo <= y <= hi]


def body(f, mat, more=0.0):
    """A real person's shapes (ANSUR II), grown by `more` (a garment's thickness). The arms hang
    a little out from the hips, palms to the thighs."""
    m = more
    f.ellipsoid(mat, (0, (TOP + CHIN) / 2, 0.01), (0.079 + m, (TOP - CHIN) / 2 + m, 0.102 + m))
    f.limb(mat, (0, NECK_BASE - 0.01, -0.012), (0, 1.6, 0.0), 0.063 + m, 0.058 + m)
    # The trunk through its measured sections: hips 0.35 × 0.25 m, waist 0.33 × 0.24, chest
    # 0.30 × 0.26 under the arms, narrowing over the shoulders to the neck.
    f.loft(mat, [(y, hw + m, hd + m, zc) for y, hw, hd, zc in TRUNK])
    for side in (-1, 1):
        x = 0.19 * side
        # The deltoids (0.52 m across them), the arms.
        f.ellipsoid(mat, (0.19 * side, SHOULDER - 0.02, 0.0), (0.054 + m, 0.075 + m, 0.056 + m))
        f.limb(mat, (x, SHOULDER, 0), (0.215 * side, ELBOW, 0), 0.054 + m, 0.042 + m)
        f.ellipsoid(mat, (0.218 * side, ELBOW - 0.08, 0.0), (0.045 + m, 0.09 + m, 0.048 + m))
        f.limb(mat, (0.215 * side, ELBOW, 0), (0.225 * side, WRIST, 0), 0.042 + m, 0.03 + m)
        # The legs: thighs 0.64 m round at the top, calves 0.40, ankles 0.235.
        hx = 0.09 * side
        f.ellipsoid(mat, (0.095 * side, 0.8, 0.0), (0.09 + m, 0.14 + m, 0.095 + m))
        f.limb(mat, (hx, HIP, 0), (hx, KNEE, 0.005), 0.098 + m, 0.058 + m)
        f.limb(mat, (hx, KNEE, 0.005), (hx, ANKLE + 0.01, -0.01), 0.052 + m, 0.036 + m)
        f.ellipsoid(mat, (hx, 0.36, -0.018), (0.058 + m, 0.11 + m, 0.062 + m))


def hands_and_feet(f, mat, more=0.0):
    m = more
    for side in (-1, 1):
        # Hands 0.198 m long and 0.09 wide; feet 0.278 long and 0.104 wide.
        f.ellipsoid(mat, (0.228 * side, (WRIST + FINGERTIPS) / 2, 0.0), (0.018 + m, 0.1 + m, 0.045 + m))
        f.ellipsoid(mat, (0.09 * side, 0.035, 0.07), (0.052 + m, 0.035 + m, 0.139 + m))


def base_figure(scene, offset):
    f = Figure(scene, offset)
    body(f, "skin")
    hands_and_feet(f, "skin")
    f.ellipsoid("hair", (0, 1.722, -0.016), (0.084, 0.088, 0.106))
    # The base layer: a vest and shorts.
    f.loft("base_layer", grown(1.02, 1.44, 0.007))
    f.loft("base_layer", grown(0.84, 1.07, 0.008))
    for side in (-1, 1):
        f.limb("base_layer", (0.09 * side, HIP, 0), (0.095 * side, 0.72, 0), 0.104, 0.096)
    return f.finish("base", soft={"skin", "base_layer"})


def outfit_figure(scene, offset):
    f = Figure(scene, offset, lift=SOLE)
    # The face, inside the hood.
    f.ellipsoid("skin", (0, (TOP + CHIN) / 2, 0.01), (0.079, (TOP - CHIN) / 2, 0.102))
    # The quilted coverall over everything but the head, hands and feet.
    body(f, "coverall", more=0.02)
    # The jacket in the player's colour, to the hips, its sleeves to the forearms.
    f.loft("jacket", grown(0.97, 1.475, 0.042))
    for side in (-1, 1):
        f.ellipsoid("jacket", (0.19 * side, SHOULDER - 0.02, 0.0), (0.082, 0.098, 0.086))
        f.limb("jacket", (0.19 * side, SHOULDER, 0), (0.215 * side, ELBOW, 0), 0.084, 0.072)
        f.limb("jacket", (0.215 * side, ELBOW, 0), (0.221 * side, 0.98, 0), 0.072, 0.066)
    # The padded hood, its face opening cut later.
    f.ellipsoid("coverall", (0, 1.69, -0.012), (0.106, 0.15, 0.127))
    # Goggles over the eyes, their strap round the hood.
    for x in (-0.033, 0.033):
        f.disc("leather", (x, EYES, 0.103), 0.029, 0.016)
        f.disc("lens", (x, EYES, 0.111), 0.023, 0.012)
    around = [math.radians(d) for d in range(60, 301, 15)]
    f.tube("leather", [(0.104 * math.sin(a), EYES + 0.012, -0.002 + 0.128 * math.cos(a)) for a in around], 0.009)
    # The belt.
    belt = [(0.208 * math.sin(a), 1.075, 0.162 * math.cos(a)) for a in [i * math.tau / 32 for i in range(33)]]
    f.tube("leather", belt, 0.022)
    for side in (-1, 1):
        x, hx = 0.228 * side, 0.09 * side
        # Thick gloves and their cuffs.
        f.ellipsoid("leather", (x, (WRIST + FINGERTIPS) / 2, 0.0), (0.03, 0.104, 0.052))
        f.limb("leather", (0.224 * side, WRIST + 0.06, 0), (0.226 * side, WRIST - 0.02, 0), 0.05, 0.047)
        # Laced boots: the foot, the shaft to mid-shin, a rubber sole.
        f.ellipsoid("leather", (hx, 0.04, 0.072), (0.064, 0.05, 0.152))
        f.limb("leather", (hx, 0.06, -0.01), (hx, 0.2, -0.008), 0.07, 0.064)
        f.ellipsoid("rubber", (hx, -0.012, 0.072), (0.066, 0.02, 0.155))
        # Leather knee and elbow pads.
        f.ellipsoid("leather", (hx, KNEE, 0.085), (0.06, 0.072, 0.03))
        f.ellipsoid("leather", (0.215 * side, ELBOW, -0.075), (0.05, 0.062, 0.026))
    # No jetpack: it is made later in the game (the user, 2026-10-21), a wearable on the back.
    parts = f.finish("outfit", soft={"skin", "coverall", "jacket"})
    # The hood's opening for the face.
    hood = next(o for o in parts if o.name.endswith("coverall"))
    bm = bmesh.new()
    bmesh.ops.create_uvsphere(bm, u_segments=32, v_segments=16, radius=1.0)
    bmesh.ops.transform(
        bm,
        matrix=Matrix.Translation(f.at((0, 1.668, 0.125))) @ Matrix.Diagonal((0.064, 0.09, 0.098, 1.0)),
        verts=bm.verts,
    )
    cut = bpy.data.objects.new("face opening", bpy.data.meshes.new("face opening"))
    bm.to_mesh(cut.data)
    bm.free()
    scene.collection.objects.link(cut)
    boolean = hood.modifiers.new("face", "BOOLEAN")
    boolean.operation = "DIFFERENCE"
    boolean.object = cut
    with bpy.context.temp_override(object=hood, active_object=hood):
        bpy.ops.object.modifier_apply(modifier="face")
    bpy.data.objects.remove(cut)
    return parts


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
    # Joining and applying modifiers work in the active scene's view layer.
    if bpy.context.window is not None:
        bpy.context.window.scene = scene
    base_figure(scene, -0.6)
    outfit_figure(scene, 0.6)
    ragdoll_boxes(scene, 0.6)
    return scene


build()
