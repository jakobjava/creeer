"""The engineer's blockout (G1.3f; ADR-0026 decision 8; docs/art-style.md §8): a real person's
body and the starting outfit, to judge the look before the detailed models. Two figures side by
side: the base body in its base layer (a vest and shorts), and the starting outfit (a quilted
coverall, a jacket in the player's colour with a belt, leather knee and elbow pads, thick
gloves, laced boots, and goggles pushed up on the forehead). The head is bare, with short hair
(the user asked for a proper face instead of a hood). The jetpack is not part of it: the player
makes it later. The rough guide is Factorio's FFF-40 engineer, without its hard hat, tools and
rucksack.

The body is Blender Studio's realistic man (Human Base Meshes, CC0), fitted to ANSUR II's men
at their means scaled to 1.80 m and posed standing (`body.py`), made an average man rather than
an athlete (`physique.py`); every garment is an offset of that body (`garments.py`), so it fits
him and moves with him. The ragdoll's segments
(bones.json, written from the Rust code) are drawn over the outfit as wire boxes: where they
disagree with the person, the segments are to be corrected (G1.3f, "within its segments").

Not a model: nothing is exported. Run it in a Blender session (it builds into its own scene,
"engineer blockout", and leaves the others alone):

    exec(open("art/engineer/blockout.py").read())

The base layer is mid grey, as the user chose; the skin and hair are one of the players' tones
each.
"""

import importlib
import json
import math
import pathlib
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector
from mathutils.geometry import convex_hull_2d

try:
    HERE = pathlib.Path(__file__).resolve().parent
except NameError:  # run by exec() in a live session
    HERE = pathlib.Path(r"C:\Users\jakmo\dev\creëer\art\engineer")
sys.path.insert(0, str(HERE))
import body as B  # noqa: E402
import garments as G  # noqa: E402
import physique as P  # noqa: E402

# A live session keeps modules between runs: the files as they are now.
B, G = importlib.reload(B), importlib.reload(G)
P = importlib.reload(P)

BONES = json.loads((HERE / "bones.json").read_text(encoding="utf-8"))

SCENE = "engineer blockout"
# The palette's tokens, sRGB; the skin is the Monk scale's sixth tone and the hair measured
# brown (two of the players' choices), the eyes' whites and irises the blockout's own.
COLOURS = {
    "skin": ("#a07e56", 0.55),
    "hair": ("#4f3f35", 0.70),
    "eye": ("#e4ddd2", 0.2),
    "iris": ("#3b2a1f", 0.1),
    "base_layer": ("#797e86", 0.85),
    "coverall": ("#8c99a6", 0.85),
    "jacket": ("#e07b39", 0.75),
    "leather": ("#4b2c22", 0.70),
    "lens": ("#2a3b33", 0.05),
    "steel": ("#8a8d90", 0.45),
    "rubber": ("#2b2a29", 0.85),
}
SOLE = 0.03  # boots' soles
# Where the garments end (m, the fitted body standing in its A-pose): the necklines (a point
# on the front of the neck and the plane's normal, tilted up at the back), the jacket's hem,
# how far up the forearm the sleeves and the gloves' cuffs reach from the wrist, the boots' tops.
NECK = ((0, -0.06, 1.47), (0, -0.474, 0.88))
HEM = 0.97
SLEEVE = 0.11
CUFF = 0.06
BOOT_TOP = 0.2
SIDES = ((1, "L"), (-1, "R"))


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
    """One figure's objects, gathered by material."""

    def __init__(self, scene):
        self.scene, self.parts = scene, {}

    def add(self, mat, obj):
        self.parts.setdefault(mat, []).append(obj)
        return obj

    def clone(self, mat, src):
        obj = src.copy()
        obj.data = src.data.copy()
        self.scene.collection.objects.link(obj)
        return self.add(mat, obj)

    def _mesh(self, mat, bm, matrix):
        bmesh.ops.transform(bm, matrix=matrix, verts=bm.verts)
        mesh = bpy.data.meshes.new(mat)
        bm.to_mesh(mesh)
        bm.free()
        for p in mesh.polygons:
            p.use_smooth = True
        obj = bpy.data.objects.new(mat, mesh)
        self.scene.collection.objects.link(obj)
        return self.add(mat, obj)

    def ellipsoid(self, mat, centre, semi):
        """An ellipsoid at `centre` with semi-axes `semi` (Blender's axes)."""
        bm = bmesh.new()
        bmesh.ops.create_uvsphere(bm, u_segments=32, v_segments=16, radius=1.0)
        return self._mesh(mat, bm, Matrix.Translation(centre) @ Matrix.Diagonal((*semi, 1.0)))

    def disc(self, mat, centre, radius, depth, facing):
        """A short cylinder (a lens, its frame) at `centre`, its axis along `facing`."""
        bm = bmesh.new()
        bmesh.ops.create_cone(bm, cap_ends=True, segments=24, radius1=radius, radius2=radius, depth=depth)
        turn = Vector((0, 0, 1)).rotation_difference(Vector(facing)).to_matrix().to_4x4()
        return self._mesh(mat, bm, Matrix.Translation(centre) @ turn)

    def tube(self, mat, points, radius, cyclic=False):
        """A strap or a rolled collar along `points`."""
        curve = bpy.data.curves.new(mat, "CURVE")
        curve.dimensions = "3D"
        curve.bevel_depth = radius
        curve.bevel_resolution = 3
        spline = curve.splines.new("POLY")
        spline.points.add(len(points) - 1)
        for p, q in zip(spline.points, points):
            p.co = (*q, 1.0)
        spline.use_cyclic_u = cyclic
        obj = bpy.data.objects.new(mat, curve)
        self.scene.collection.objects.link(obj)
        return self.add(mat, obj)

    def band(self, mat, points, half_height, thickness, filled=False):
        """A flat band (a belt) round the closed loop `points`, all at one height; `filled`, a
        slab (a sole) with rounded edges."""
        curve = bpy.data.curves.new(mat, "CURVE")
        curve.dimensions = "2D"
        curve.fill_mode = "BOTH" if filled else "NONE"
        curve.extrude = half_height
        curve.bevel_depth = thickness / 2
        spline = curve.splines.new("POLY")
        spline.points.add(len(points) - 1)
        for p, q in zip(spline.points, points):
            p.co = (q.x, q.y, 0.0, 1.0)
        spline.use_cyclic_u = True
        obj = bpy.data.objects.new(mat, curve)
        obj.location.z = points[0].z
        self.scene.collection.objects.link(obj)
        return self.add(mat, obj)

    def pose(self):
        for objs in self.parts.values():
            for obj in objs:
                if obj.type == "MESH":
                    B.pose(obj)

    def finish(self, name, at):
        """Joins each material's parts into one object, standing at `at`."""
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
            obj.data.transform(obj.matrix_world)
            obj.matrix_world = Matrix.Identity(4)
            obj.name = f"{name} {mat}"
            if mat != "eye":
                obj.data.materials.clear()
                obj.data.materials.append(material(mat))
            obj.location = at
            out.append(obj)
        return out


def head(f, skin, eyes):
    """The skin (all of it: the clothes cover the rest), the eyeballs with their irises (the
    faces looking ahead within 28°: an 11.7 mm iris on a 24 mm eye), short hair."""
    f.clone("skin", skin)
    for src in eyes:
        obj = f.clone("eye", src)
        obj.data.materials.clear()
        obj.data.materials.append(material("eye"))
        obj.data.materials.append(material("iris"))
        centre = sum((v.co for v in obj.data.vertices), Vector()) / len(obj.data.vertices)
        for p in obj.data.polygons:
            ahead = (p.center - centre).normalized().dot(Vector((0, -1, 0)))
            p.material_index = 1 if ahead > math.cos(math.radians(28)) else 0
    f.add("hair", G.hair(f.scene, skin, G.head_centre(skin, eyes)))


def forearm(side):
    """The wrist and the direction up the forearm, fitted (A-pose)."""
    w, e = B.joint("wrist", side), B.joint("elbow", side)
    return w, (e - w).normalized()


def on_arm(cloth, tag):
    return lambda v: cloth.weight(v, "w_arm." + tag) > 0.5


def base_figure(scene, skin, eyes):
    f = Figure(scene)
    head(f, skin, eyes)
    # The base layer: a vest with short sleeves, and shorts.
    vest = G.Cloth(skin).cut(*NECK)
    vest.cut((0, 0, 1.0), (0, 0, -1), where=lambda v: vest.arm(v) < 0.5)
    for side, tag in SIDES:
        s, e = B.joint("shoulder", side), B.joint("elbow", side)
        vest.cut(s + (e - s).normalized() * 0.12, e - s, where=on_arm(vest, tag))
    f.add("base_layer", vest.make(scene, "vest", 0.004, over=skin, gap=0.002, passes=20))
    shorts = G.Cloth(skin)
    shorts.drop(lambda v: shorts.arm(v) > 0.5).cut((0, 0, 1.07), (0, 0, 1)).cut((0, 0, 0.72), (0, 0, -1))
    f.add("base_layer", shorts.make(scene, "shorts", 0.006, over=skin, gap=0.003, passes=30, hang=(G.SEAT, 400)))
    f.pose()
    return f.finish("base", (-0.6, 0, 0))


def outfit_figure(scene, skin, eyes):
    f = Figure(scene)
    head(f, skin, eyes)
    # The quilted coverall over everything but the head, hands and feet: 6 mm of padding, 8 mm
    # of ease (the user, 2026-10-21: twice "too thick"). The jacket over it adds 8 mm more.
    c = G.Cloth(skin).cut(*NECK).cut((0, 0, 0.16), (0, 0, -1))
    for side, tag in SIDES:
        w, up = forearm(side)
        c.cut(w, -up, where=on_arm(c, tag))
    coverall = f.add("coverall", c.make(scene, "coverall", 0.014, over=skin, gap=0.006, passes=200, hang=(G.TRUNK, 500)))
    # The jacket in the player's colour, to the hips, its sleeves to the forearms, with a stand
    # collar round the neck (the user, 2026-10-21: no rolled "sausage"); it falls from the chest.
    j = G.Cloth(skin).cut(*NECK)
    j.cut((0, 0, HEM), (0, 0, -1), where=lambda v: j.arm(v) < 0.5)
    for side, tag in SIDES:
        w, up = forearm(side)
        j.cut(w + up * SLEEVE, -up, where=on_arm(j, tag))
    jacket = f.add("jacket", j.make(scene, "jacket", 0.022, over=coverall, gap=0.004, passes=300, hang=(G.TRUNK, 500)))
    neckline = G.smoothed(G.around([p for p in j.rim if p.z > 1.3 and abs(p.x) < 0.12]))
    f.add("jacket", G.collar(scene, "collar", neckline, skin))
    ring, middle = G.ring(jacket, 1.075, lambda co: abs(co.x) < 0.25)
    f.band("leather", [middle + (p - middle) * 1.02 for p in ring], 0.02, 0.005)
    # Thick gloves to past the wrists, and laced boots to mid-shin.
    gl = G.Cloth(skin)
    gl.keep(lambda v: gl.arm(v) > 0.5)
    for side, tag in SIDES:
        w, up = forearm(side)
        gl.cut(w + up * CUFF, up, where=on_arm(gl, tag))
    f.add("leather", gl.make(scene, "gloves", 0.004, passes=20))
    bo = G.Cloth(skin)
    bo.keep(lambda v: bo.arm(v) < 0.5 and v.co.z < BOOT_TOP + 0.05).cut((0, 0, BOOT_TOP), (0, 0, 1)).hull()
    f.add("leather", bo.make(scene, "boots", 0.012, over=skin, gap=0.008, passes=10, subdivide=2))
    goggles(f, skin)
    f.pose()
    pads_and_soles(f, coverall, jacket)
    return f.finish("outfit", (0.6, 0, SOLE))


def goggles(f, skin):
    """Goggles pushed up on the forehead, their strap round the head: the face stays open."""
    hair = f.parts["hair"][0]
    facing = Vector((0, -1, 0.55)).normalized()
    for x in (-0.034, 0.034):
        near = [v.co for o in (skin, hair) for v in o.data.vertices if abs(v.co.x - x) < 0.006 and abs(v.co.z - 1.752) < 0.006]
        front = min(c.y for c in near)
        f.disc("leather", Vector((x, front - 0.004, 1.752)), 0.028, 0.016, facing)
        f.disc("lens", Vector((x, front - 0.011, 1.756)), 0.022, 0.012, facing)
    ring, middle = G.ring(hair, 1.748)
    back = [middle + (p - middle) * 1.04 for p in ring if abs(math.degrees(math.atan2(p.x - middle.x, middle.y - p.y))) > 55]
    f.tube("leather", sorted(back, key=lambda p: math.atan2(p.x - middle.x, p.y - middle.y)), 0.007)


def pads_and_soles(f, coverall, jacket):
    """Leather knee and elbow pads and the boots' rubber soles, where the posed body has its
    knees, elbows and feet."""
    boots = next(o for o in f.parts["leather"] if o.name.startswith("boots"))
    for side, tag in SIDES:
        knee = B.posed(B.joint("knee", side), side, "leg")
        f.add("leather", G.pad(f.scene, "knee pad", coverall, knee + Vector((0, 0, 0.01)), 0.055, 0.07, (0, -1, 0), "w_leg." + tag))
        elbow = B.posed(B.joint("elbow", side), side, "arm")
        f.add("leather", G.pad(f.scene, "elbow pad", jacket, elbow, 0.045, 0.06, (0, 1, 0), "w_arm." + tag))
        # The sole: the boot's outline seen from below, 5 mm wider, 3 cm thick.
        foot = [v.co for v in boots.data.vertices if v.co.x * side > 0 and v.co.z < 0.06]
        hull = [foot[i] for i in convex_hull_2d([(c.x, c.y) for c in foot])]
        middle = sum(hull, Vector()) / len(hull)
        outline = [middle + (c - middle) * (1.0 + 0.005 / (c - middle).xy.length) for c in hull]
        f.band("rubber", [Vector((c.x, c.y, -SOLE / 2)) for c in outline], SOLE / 2 - 0.004, 0.008, filled=True)


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
    skin, eyes = B.load(scene)
    B.weigh(skin)
    for obj in (skin, *eyes):
        B.fit(obj)
    for name, value in P.average(skin).items():
        print(f"art: {name}: {value}")
    base_figure(scene, skin, eyes)
    outfit_figure(scene, skin, eyes)
    for obj in (skin, *eyes):
        bpy.data.objects.remove(obj)
    P.measure(scene.objects["base skin"])
    ragdoll_boxes(scene, 0.6)
    return scene


build()
