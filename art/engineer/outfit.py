"""The engineer's pieces (G1.3f; ADR-0026 decisions 7 and 8): the base body and each wearable of
the starting outfit, built from the fitted body (`body.py`, `physique.py`) as offsets of it
(`garments.py`), posed standing and lifted onto the boots' 3 cm soles: the ragdoll's frame
(`creeer_sim::character::ragdoll`). Each piece is a list of objects in palette tokens' materials:

- `body`: the skin, the eyes (`eye_white`, `iris`) and the hair;
- `base_layer`: a vest with short sleeves and shorts, the "naked" body's;
- `coverall`, `jacket` (with its stand collar, in `player_accent`), `belt`, `gloves`, `boots`
  (leather on rubber soles), `knee_pads`, `elbow_pads` and `goggles` (pushed up on the
  forehead): the starting outfit.

`blockout.py` shows them; `models.py` details, weights and exports them.
"""

import math

import bmesh
import bpy
import body as B
import garments as G
import goggles
import physique as P
from mathutils import Matrix, Vector
from mathutils.geometry import convex_hull_2d

SOLE = 0.03  # boots' soles
# Where the garments end (m, the fitted body standing in its A-pose): the necklines (a point
# on the front of the neck and the plane's normal, tilted up at the back), the jacket's hem,
# how far up the forearm the sleeves and the gloves' cuffs reach from the wrist, the boots' tops.
NECK = ((0, -0.06, 1.47), (0, -0.474, 0.88))
HEM = 0.97
SLEEVE = 0.11
CUFF = 0.06
BOOT_TOP = 0.2
# The coverall's legs' ends, inside the boots' shafts; the vest's neckline below the coverall's
# (m), so it lies under it.
LEG_END = 0.16
VEST_DROP = 0.03
# How far within a garment's opening the body's hidden regions begin (m): looking into the
# collar, a sleeve's end or a trouser leg, the skin is there.
INSIDE = {"neck": 0.02, "hand": 0.035, "foot": 0.03}
SIDES = ((1, "L"), (-1, "R"))
# How far the trouser legs and the sleeves come back in going down from their widest point (m per
# m): they hang nearly straight (`hang`).
LEG_SLOPE, ARM_SLOPE = 0.05, 0.06
# The per-player tokens as previewed: the Monk scale's sixth tone, measured brown hair, player
# one's orange (`art-style.md` §3.3, §3.6).
PREVIEW = {"skin": "#a07e56", "hair": "#4f3f35", "player_accent": "#e07b39"}
# The player's colour as Factorio lays it on its engineer (FFF-218, art-style.md §1b): a tint at
# half strength over a desaturated cloth, so it reads muted and worn, never hi-vis.
PLAYER_BASE, PLAYER_STRENGTH = "#5c5a57", 0.5


def srgb(hexcode):
    c = [int(hexcode[i : i + 2], 16) / 255 for i in (1, 3, 5)]
    return tuple(x / 12.92 if x <= 0.04045 else ((x + 0.055) / 1.055) ** 2.4 for x in c)


def preview_colour(token):
    """The linear colour a per-player token is previewed in (player one's)."""
    rgb = srgb(PREVIEW[token])
    if token == "player_accent":
        base = srgb(PLAYER_BASE)
        rgb = tuple(b + (c - b) * PLAYER_STRENGTH for b, c in zip(base, rgb))
    return rgb


def token(name):
    """The material named after palette token `name`, previewed in its colour."""
    mat = bpy.data.materials.get(name)
    if mat is None:
        import common

        rgb, rough, metal = common.palette()[name][:3]
        if name in PREVIEW:
            rgb = preview_colour(name)
        mat = bpy.data.materials.new(name)
        mat.use_nodes = True
        bsdf = next(n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
        bsdf.inputs["Base Color"].default_value = (*rgb, 1.0)
        bsdf.inputs["Roughness"].default_value = rough
        bsdf.inputs["Metallic"].default_value = metal
        mat.diffuse_color = (*rgb, 1.0)
    return mat


class Pieces:
    """The pieces' objects, by piece."""

    def __init__(self, scene):
        self.scene, self.objects = scene, {}

    def add(self, piece, obj, material=None):
        if material is not None and obj.type == "MESH":
            obj.data.materials.clear()
            obj.data.materials.append(token(material))
        elif material is not None:
            obj.data.materials.append(token(material))
        self.objects.setdefault(piece, []).append(obj)
        return obj

    def clone(self, piece, src, material=None):
        obj = src.copy()
        obj.data = src.data.copy()
        self.scene.collection.objects.link(obj)
        return self.add(piece, obj, material)

    def mesh(self, piece, name, bm, matrix, material):
        bmesh.ops.transform(bm, matrix=matrix, verts=bm.verts)
        mesh = bpy.data.meshes.new(name)
        bm.to_mesh(mesh)
        bm.free()
        for p in mesh.polygons:
            p.use_smooth = True
        obj = bpy.data.objects.new(name, mesh)
        self.scene.collection.objects.link(obj)
        return self.add(piece, obj, material)

    def disc(self, piece, name, centre, radius, depth, facing, material):
        """A short cylinder (a lens, its frame) at `centre`, its axis along `facing`."""
        bm = bmesh.new()
        bmesh.ops.create_cone(bm, cap_ends=True, segments=32, radius1=radius, radius2=radius, depth=depth)
        turn = Vector((0, 0, 1)).rotation_difference(Vector(facing)).to_matrix().to_4x4()
        return self.mesh(piece, name, bm, Matrix.Translation(centre) @ turn, material)

    def curve(self, piece, name, points, material, radius=0.0, cyclic=False, flat=None, filled=False, closed=True):
        """A strap along `points` (round, `radius`), or a flat band along `points` at one height
        (`flat` = (half height, thickness)), round them if `closed`, filled as a slab."""
        curve = bpy.data.curves.new(name, "CURVE")
        spline = curve.splines.new("POLY")
        spline.points.add(len(points) - 1)
        if flat is None:
            curve.dimensions = "3D"
            curve.bevel_depth = radius
            curve.bevel_resolution = 3
            for p, q in zip(spline.points, points):
                p.co = (*q, 1.0)
        else:
            curve.dimensions = "2D"
            curve.fill_mode = "BOTH" if filled else "NONE"
            curve.extrude, curve.bevel_depth = flat[0], flat[1] / 2
            for p, q in zip(spline.points, points):
                p.co = (q.x, q.y, 0.0, 1.0)
        spline.use_cyclic_u = cyclic or (flat is not None and closed)
        obj = bpy.data.objects.new(name, curve)
        if flat is not None:
            obj.location.z = points[0].z
        self.scene.collection.objects.link(obj)
        return self.add(piece, obj, material)

    def meshes(self):
        """Every curve made a mesh, its transform baked in."""
        for piece, objs in self.objects.items():
            out = []
            for obj in objs:
                if obj.type != "MESH":
                    with bpy.context.temp_override(selected_editable_objects=[obj], active_object=obj, object=obj):
                        bpy.ops.object.convert(target="MESH")
                    obj = self.scene.objects[obj.name]
                    # A curve's mesh comes in pieces (its caps apart from its sides): merged,
                    # or decimation keeps every edge between them as an open one.
                    bm = bmesh.new()
                    bm.from_mesh(obj.data)
                    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
                    bm.to_mesh(obj.data)
                    bm.free()
                obj.data.transform(obj.matrix_world)
                obj.matrix_world = Matrix.Identity(4)
                for p in obj.data.polygons:
                    p.use_smooth = True
                out.append(obj)
            self.objects[piece] = out


def forearm(side):
    """The wrist and the direction up the forearm, fitted (A-pose)."""
    w, e = B.joint("wrist", side), B.joint("elbow", side)
    return w, (e - w).normalized()


def openings():
    """The planes the body's regions part at (`regions.py`; posed, on the soles), each `INSIDE`
    (a sleeve's 3.5 cm: from 2 cm in, the arms hanging forward, a look in past the cuff's wide
    mouth found the hidden forearm) within a garment's opening: the neck's (the coverall's and the jacket's neckline), each
    wrist's (the coverall's sleeve) and each ankle's (its leg). Named by the region beyond, each
    a point and the normal pointing out of the garment: the skin on its side is that region's."""
    lift = Vector((0.0, 0.0, SOLE))
    co, no = Vector(NECK[0]), Vector(NECK[1]).normalized()
    out = {"neck": (co + lift - no * INSIDE["neck"], no)}
    for side, tag in SIDES:
        w, up = forearm(side)
        a = B.posed(w, side, "forearm")
        up = (B.posed(w + up, side, "forearm") - a).normalized()
        out["hand." + tag] = (a + lift + up * INSIDE["hand"], -up)
        ankle = B.posed(B.joint("foot", side), side, "leg")
        out["foot." + tag] = (Vector((ankle.x, ankle.y, LEG_END + SOLE + INSIDE["foot"])), Vector((0.0, 0.0, -1.0)))
    return out


def on_arm(cloth, tag):
    return lambda v: cloth.weight(v, "w_arm." + tag) > 0.5


def body(p, skin, eyes):
    """The skin (all of it: what is hidden is the regions' business), the eyeballs with their
    irises (the faces looking ahead within 28°: an 11.7 mm iris on a 24 mm eye), short hair
    and eyebrows (`hair.py`)."""
    p.clone("body", skin, "skin").name = "skin"
    for src, side in zip(eyes, "LR"):
        obj = p.clone("body", src)
        obj.name = f"eye.{side}"
        obj.data.materials.clear()
        obj.data.materials.append(token("eye_white"))
        obj.data.materials.append(token("iris"))
        centre = sum((v.co for v in obj.data.vertices), Vector()) / len(obj.data.vertices)
        for f in obj.data.polygons:
            ahead = (f.center - centre).normalized().dot(Vector((0, -1, 0)))
            f.material_index = 1 if ahead > math.cos(math.radians(28)) else 0
    import hair as H

    centre = G.head_centre(skin, eyes)
    p.add("body", H.haircut(p.scene, skin, centre), "hair").name = "hair"
    p.add("body", H.brows(p.scene, skin, eyes), "hair").name = "brows"


def base_layer(p, skin):
    # The neckline cut about the neck alone: the shoulders' tops rise above its plane, and cut
    # whole it left a round hole in each (G1.3e, seen once the game drew the base layer); cut
    # within 0.145 m of the middle, its outline stepped up where the cut stopped, a notch over
    # each collarbone (G1.3i). Only the piece round the neck goes.
    neck = Vector(NECK[0]) - Vector(NECK[1]).normalized() * VEST_DROP
    vest = G.Cloth(skin).cut(neck, NECK[1], through=(0, 0, 1.7))
    vest.cut((0, 0, 1.0), (0, 0, -1), where=lambda v: vest.arm(v) < 0.5)
    for side, tag in SIDES:
        s, e = B.joint("shoulder", side), B.joint("elbow", side)
        vest.cut(s + (e - s).normalized() * 0.12, e - s, where=on_arm(vest, tag))
    p.add("base_layer", vest.make(p.scene, "vest", 0.004, over=skin, gap=0.002, passes=20), "base_layer")
    shorts = G.Cloth(skin)
    shorts.drop(lambda v: shorts.arm(v) > 0.5).cut((0, 0, 1.07), (0, 0, 1)).cut((0, 0, 0.72), (0, 0, -1))
    p.add("base_layer", shorts.make(p.scene, "shorts", 0.006, over=skin, gap=0.003, passes=30, hang=(G.SEAT, 400)), "base_layer")


def clothes(p, skin):
    """The coverall (6 mm of padding, 8 mm of ease: the user, twice "too thick"), the jacket
    over it with its stand collar (the user: no rolled "sausage"), the belt, the gloves and the
    boots. Returns the coverall and the jacket, which the pads go on."""
    c = G.Cloth(skin).cut(*NECK).cut((0, 0, LEG_END), (0, 0, -1))
    for side, tag in SIDES:
        w, up = forearm(side)
        c.cut(w, -up, where=on_arm(c, tag))
    coverall = p.add("coverall", c.make(p.scene, "coverall", 0.014, over=skin, gap=0.006, passes=200, hang=(G.TRUNK, 500)), "coverall")
    j = G.Cloth(skin).cut(*NECK)
    j.cut((0, 0, HEM), (0, 0, -1), where=lambda v: j.arm(v) < 0.5)
    for side, tag in SIDES:
        w, up = forearm(side)
        j.cut(w + up * SLEEVE, -up, where=on_arm(j, tag))
    jacket = p.add("jacket", j.make(p.scene, "jacket", 0.022, over=coverall, gap=0.004, passes=300, hang=(G.TRUNK, 500)), "player_accent")
    neckline = G.smoothed(G.around([q for q in j.rim if q.z > 1.3 and abs(q.x) < 0.12]))
    p.add("jacket", G.collar(p.scene, "collar", neckline, skin), "coverall")
    ring, middle = G.ring(jacket, 1.075, lambda co: abs(co.x) < 0.25)
    p.curve("belt", "belt", [middle + (q - middle) * 1.02 for q in ring], "leather", flat=(0.02, 0.005))
    gl = G.Cloth(skin)
    gl.keep(lambda v: gl.arm(v) > 0.5)
    for side, tag in SIDES:
        w, up = forearm(side)
        gl.cut(w + up * CUFF, up, where=on_arm(gl, tag))
    p.add("gloves", gl.make(p.scene, "gloves", 0.004, passes=20), "leather")
    bo = G.Cloth(skin)
    bo.keep(lambda v: bo.arm(v) < 0.5 and v.co.z < BOOT_TOP + 0.05).cut((0, 0, BOOT_TOP), (0, 0, 1)).hull()
    p.add("boots", bo.make(p.scene, "boots", 0.012, over=skin, gap=0.008, passes=10), "leather")
    return coverall, jacket


def hang(p):
    """The coverall's legs and both garments' sleeves hung from what holds them up (posed, so
    they fall along the limbs: `relief.hang`), the jacket then kept over the coverall."""
    import relief as R

    coverall, jacket = p.objects["coverall"][0], p.objects["jacket"][0]
    for side, tag in SIDES:
        leg = R.Limb((f"hip.{tag}", f"knee.{tag}", f"ankle.{tag}"), side, drop=SOLE)
        arm = R.Limb((f"shoulder.{tag}", f"elbow.{tag}", f"wrist.{tag}"), side, drop=SOLE)
        R.hang(coverall.data, leg, R.weights(coverall.data, (f"thigh.{tag}", f"shank.{tag}")), LEG_SLOPE, start=0.12)
        for obj in (coverall, jacket):
            R.hang(obj.data, arm, R.weights(obj.data, (f"upper_arm.{tag}", f"forearm.{tag}")), ARM_SLOPE, start=0.1)
    R.over(jacket, coverall, 0.004)


def pads_and_soles(p, coverall, jacket):
    """Leather knee and elbow pads and the boots' rubber soles, where the posed body has its
    knees, elbows and feet."""
    boots = p.objects["boots"][0]
    for side, tag in SIDES:
        knee = B.posed(B.joint("knee", side), side, "leg")
        p.add("knee_pads", G.pad(p.scene, "knee pad", coverall, knee + Vector((0, 0, 0.01)), 0.055, 0.07, (0, -1, 0), "w_leg." + tag, off=0.01), "leather")
        elbow = B.posed(B.joint("elbow", side), side, "arm")
        p.add("elbow_pads", G.pad(p.scene, "elbow pad", jacket, elbow, 0.045, 0.06, (0, 1, 0), "w_arm." + tag, thick=0.003), "leather")
        # The sole: the boot's outline seen from below, 5 mm wider, 3 cm thick.
        foot = [v.co for v in boots.data.vertices if v.co.x * side > 0 and v.co.z < 0.06]
        hull = [foot[i] for i in convex_hull_2d([(c.x, c.y) for c in foot])]
        middle = sum(hull, Vector()) / len(hull)
        outline = [middle + (c - middle) * (1.0 + 0.005 / (c - middle).xy.length) for c in hull]
        p.curve("boots", "sole", [Vector((c.x, c.y, -SOLE / 2)) for c in outline], "rubber", flat=(SOLE / 2 - 0.004, 0.008), filled=True)


def fitted(scene):
    """The base mesh's body and eyes appended into `scene`, fitted to the person (A-pose), and
    what `physique.average` measured and fitted."""
    skin, eyes = B.load(scene)
    B.weigh(skin)
    for obj in (skin, *eyes):
        B.fit(obj)
    return skin, eyes, P.average(skin)


def build(scene, skin, eyes):
    """Every piece from the fitted body `skin` and `eyes` (A-pose), posed and on the soles; the
    segments' weights first (on the A-pose, arms clear of the trunk), for every piece to carry."""
    B.weigh_segments(skin)
    p = Pieces(scene)
    body(p, skin, eyes)
    base_layer(p, skin)
    coverall, jacket = clothes(p, skin)
    goggles.goggles(p, skin)
    for objs in p.objects.values():
        for obj in objs:
            if obj.type == "MESH":
                B.pose(obj)
    hang(p)
    pads_and_soles(p, coverall, jacket)
    p.meshes()
    lift = Matrix.Translation((0.0, 0.0, SOLE))
    for objs in p.objects.values():
        for obj in objs:
            obj.data.transform(lift)
    return p.objects
