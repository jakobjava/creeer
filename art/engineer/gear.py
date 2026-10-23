"""The gear's detail (G1.3f; `detail.py`): the gloves, the belt, the pads and the goggles, as
workwear and 1930s welding goggles are made (ADR-0026's detail survey). In the ragdoll's frame:

- **gloves**: a strap round each wrist with a steel buckle on its back, a knuckle guard raised
  over the back of the hand;
- **belt**: a steel buckle in front with its prong, the strap's end through a keeper beside it,
  a leather pouch on the right hip, behind the hanging arm;
- **knee and elbow pads**: the knees' held by a strap behind each knee, buckled in steel on its
  outer side; the elbows' sewn on;
- **goggles**: steel rims round the green lenses and steel side shields round the leather
  cups (American Optical's 1930s welding goggles), a bridge between them.

`boots.py` details the boots.
"""

import math

import bmesh
import numpy as np
from mathutils import Matrix, Vector

import detail as D
import garments as G
import relief as R
import trims as T

SIDES = ((1, "L"), (-1, "R"))
UP = Vector((0, 0, 1))
# The pouch: which way round the waist (from the front, towards the right, rad), its width,
# height and depth (m).
POUCH = (math.radians(130), 0.10, 0.11, 0.04)


def hand(tag, side):
    """The forearm and hand as a limb: elbow, wrist, and the fingertips' way."""
    wrist = R.joint(f"wrist.{tag}")
    centre = R.segment(f"hand.{tag}")
    return R.Limb((f"elbow.{tag}", f"wrist.{tag}", wrist + 2 * (centre - wrist)), side)


def strap(p, piece, host, limb, t, only, width=0.02, buckle=True, bins=32, on=None):
    """A leather strap round `host` where it crosses `limb` at `t` (m along) (round its part on
    the segments `only`) through `bins` points, with a steel buckle on its outer side; weighted
    as `host` is on the segments `on` (`only` by default)."""
    held = R.weights(host.data, only)
    on = on or only
    points, normals, axis, radius = T.loop_round(host, limb, t, held, gap=0.0015, bins=bins, width=width)
    T.cinch_round(host, limb, t, held, radius, width)
    bm = bmesh.new()
    T.band(bm, points, normals, [axis] * len(points), width, 0.0025)
    if buckle:
        k = min(range(len(points)), key=lambda i: abs(math.atan2(normals[i].dot(Vector(limb.frames[0][1])), normals[i].dot(Vector(limb.frames[0][0])))))
        along = (points[(k + 1) % len(points)] - points[k - 1]).normalized()
        metal = bmesh.new()
        T.buckle(metal, points[k] + normals[k] * 0.0025, normals[k], along, width * 1.1, width * 1.3, tube_radius=0.0018)
        T.place(p, piece, "buckle", metal, "steel", host, on)["hardware"] = "close"
        # The strap's end through the buckle, hanging free past it.
        n = normals[k]
        # Drooping in a smooth curve (three points bent it 25° at one edge, 1.1 mm off its curve).
        end = [points[k] + n * (0.004 + 0.001 * s) + along * (0.034 * s) - UP * (0.006 * s * s) for s in np.linspace(0.0, 1.0, 6)]
        T.band(bm, end, [n] * len(end), [axis] * len(end), width * 0.9, 0.0025, closed=False)
    obj = T.place(p, piece, "strap", bm, "leather", host, on)
    obj["hardware"] = "whole"
    T.sharpen(obj.data, 50)


def gloves(p):
    obj = p.objects["gloves"][0]
    co, no = R.arrays(obj.data)
    lift = np.zeros(len(co))
    for side, tag in SIDES:
        limb = hand(tag, side)
        t, theta, _ = limb.coords(co)
        wrist = limb.lengths[0]
        back = R.smoothstep(0.35, 0.6, no @ np.array((side, 0.0, 0.0)))
        span = R.smoothstep(wrist + 0.07, wrist + 0.078, t) * (1.0 - R.smoothstep(wrist + 0.1, wrist + 0.108, t))
        lift += (side * co[:, 0] > 0) * back * span * 0.002
    R.displace(obj.data, lift)
    for side, tag in SIDES:
        limb = hand(tag, side)
        strap(p, "gloves", obj, limb, limb.lengths[0] + 0.018, (f"hand.{tag}",), width=0.016)


def belt(p, belt_z):
    obj = p.objects["belt"][0]
    surface = T.Surface(obj)
    front, normal = surface.cast((0, -0.4, belt_z), (0, 1, 0))
    metal = bmesh.new()
    T.buckle(metal, front + normal * 0.004, normal, Vector((1, 0, 0)), 0.05, 0.054, tube_radius=0.0035, fine=True)
    T.place(p, "belt", "buckle", metal, "steel", obj)["hardware"] = "mid"
    # The strap's end through the keeper, its last few centimetres off the belt and drooping.
    tail, ns = [], []
    for x in np.linspace(0.006, 0.11, 9):
        hit, n = surface.cast((x, -0.4, belt_z), (0, 1, 0))
        free = max(0.0, (x - 0.075) / 0.035)
        tail.append(hit + n * (0.0005 + 0.003 * free) - UP * (0.012 * free * free))
        ns.append(n)
    bm = bmesh.new()
    T.band(bm, tail, ns, [UP] * len(tail), 0.034, 0.004, closed=False)
    # The keeper round the belt (5 mm) and the strap's end on it, its back 1.5 mm behind the belt
    # (12 mm behind its face, the back went 4 mm into the jacket gathered beneath).
    keeper = tail[5] + ns[5] * 0.001
    loop = [keeper + ns[5] * 0.006 + UP * 0.024, keeper + ns[5] * 0.006 - UP * 0.024, keeper - ns[5] * 0.0075 - UP * 0.024, keeper - ns[5] * 0.0075 + UP * 0.024]
    outs = [ns[5], ns[5], -ns[5], -ns[5]]
    T.band(bm, [loop[0], (loop[0] + loop[1]) / 2, loop[1], (loop[1] + loop[2]) / 2, loop[2], (loop[2] + loop[3]) / 2, loop[3], (loop[3] + loop[0]) / 2],
           [outs[0], outs[0], outs[0], -UP, outs[2], outs[2], outs[2], UP], [Vector((1, 0, 0))] * 8, 0.012, 0.002)
    T.sharpen(T.place(p, "belt", "tail", bm, "leather", obj).data, 50)
    angle, width, height, depth = POUCH
    way = Vector((-math.sin(angle), -math.cos(angle), 0))
    hit, n = surface.cast(Vector((0, 0, belt_z)) + way * 0.4, -way)
    n = Vector((n.x, n.y, 0)).normalized()
    across = UP.cross(n)
    top = belt_z + 0.022
    # Its back 3 mm off the cloth behind it, however far the jacket puffs out below the belt
    # (5 mm off the belt, the jacket came 7 mm through it).
    cloth = T.Surface(next(o for o in p.objects["jacket"] if o.name.startswith("jacket")))
    back = 0.005
    for u in np.linspace(-width / 2, width / 2, 6):
        for v in np.linspace(top - height - belt_z, top - belt_z, 8):
            q, _ = cloth.cast(hit + across * u + UP * v + n * 0.1, -n)
            if q is not None:
                back = max(back, (q - hit).dot(n) + 0.003)
    basis = Matrix((across, n, UP)).transposed().to_4x4()
    bm = bmesh.new()
    box(bm, Matrix.Translation(hit + n * (back + depth / 2) + UP * (top - height / 2 - belt_z)) @ basis, (width, depth, height), 0.009)
    box(bm, Matrix.Translation(hit + n * (back + 0.001 + depth) + UP * (top - 0.025 - belt_z)) @ basis, (width + 0.004, 0.006, 0.05), 0.003)
    # Weighted as the jacket's hip it hangs on (as the belt, it stayed while the thigh turned
    # the cloth behind it 10 mm through it).
    jacket = next(o for o in p.objects["jacket"] if o.name.startswith("jacket"))
    hips = ("pelvis", "torso", "thigh.L", "thigh.R")
    T.place(p, "belt", "pouch", bm, "leather", jacket, hips)
    metal = bmesh.new()
    T.dome(metal, hit + n * (back + 0.004 + depth) + UP * (top - 0.045 - belt_z), n, 0.007, 0.003)
    T.place(p, "belt", "snap", metal, "steel", jacket, hips)["hardware"] = "close"


def box(bm, matrix, size, bevel):
    """A box of `size` (m) with rounded edges (`bevel`, m), placed by `matrix`, added to `bm`."""
    import bpy

    one = bmesh.new()
    bmesh.ops.create_cube(one, size=1.0)
    bmesh.ops.scale(one, vec=Vector(size), verts=one.verts)
    bmesh.ops.bevel(one, geom=list(one.verts) + list(one.edges), offset=bevel, segments=2, affect="EDGES", profile=0.5)
    bmesh.ops.transform(one, matrix=matrix, verts=one.verts)
    mesh = bpy.data.meshes.new("box")
    one.to_mesh(mesh)
    one.free()
    bm.from_mesh(mesh)
    bpy.data.meshes.remove(mesh)


def stand_off(pad, host, gap):
    """`pad` moved out whole along its facing until no point of it lies nearer `host` than `gap`
    (m)."""
    surface = T.Surface(host)
    facing = sum((f.normal for f in pad.data.polygons), Vector()).normalized()
    need = 0.0
    for v in pad.data.vertices:
        q, n = surface.nearest(v.co)
        need = max(need, gap - (v.co - q).dot(n))
    if need > 0.0:
        for v in pad.data.vertices:
            v.co += facing * need
        pad.data.update()


def pads(p):
    coverall = next(o for o in p.objects["coverall"] if o.name.startswith("coverall"))
    jacket = next(o for o in p.objects["jacket"] if o.name.startswith("jacket"))
    for piece, host, limbs in (("knee_pads", coverall, D.leg), ("elbow_pads", jacket, D.arm)):
        # Plain leather: ribs pressed into them came out as jagged slits once decimated.
        for pad in list(p.objects[piece]):
            R.subdivide(pad, 1)
            if piece == "knee_pads":
                # A stiff plate: cut from the coverall, it kept its undulations, and kept to
                # §7's millimetre took 700 triangles a pad.
                G.soften(pad.data, 40)
            # Kept over the folds and patches the host gained after the pad was laid on it (the
            # coverall's knee came 4 mm through it): a knee pad moved out whole, as stiff leather
            # rests on the highest of them (pushed out point by point, its tip crumpled over the
            # knee patch's step, facets 2 mm off their curve); an elbow's patch, sewn on, follows.
            (stand_off if piece == "knee_pads" else R.over)(pad, host, pad["off"])
        for side, tag in SIDES:
            limb = limbs(side, tag)
            joint = limb.lengths[0]
            only = tuple(f"{s}.{tag}" for s in (("thigh", "shank") if piece == "knee_pads" else ("upper_arm", "forearm")))
            # One strap behind each knee, round the calf's top under the knee's pit (at the joint,
            # it crumpled into the fold of the knee bent to its limit), held by the shank alone (as
            # the cloth beneath, a third the thigh's, the knee bent fully put that cloth 9.5 mm
            # through it); the elbows' patches are sewn on.
            for dt in ((0.05,) if piece == "knee_pads" else ()):
                strap(p, piece, host, limb, joint + dt, only, on=(f"shank.{tag}",))


def goggles(p):
    objs = p.objects["goggles"]
    facing = Vector((0, -1, 0.55)).normalized()
    metal, bm = bmesh.new(), bmesh.new()
    frames = sorted((o for o in objs if o.name.startswith("frame")), key=lambda o: sum(v.co.x for v in o.data.vertices))
    for obj in objs:
        centre = sum((v.co for v in obj.data.vertices), Vector()) / len(obj.data.vertices)
        if obj.name.startswith("lens"):
            T.ring(metal, centre + facing * 0.006, facing, 0.0225, 0.0028, segments=20, sides=6)
        elif obj.name.startswith("frame"):
            T.ring(metal, centre, facing, 0.0285, 0.0035, segments=20, sides=6)
    a, b = (sum((v.co for v in o.data.vertices), Vector()) / len(o.data.vertices) for o in frames)
    span = (b - a).normalized()
    # Arched clear of the hair over the brow (at the cups' height it went through it).
    T.tube(bm, [a + span * 0.027, (a + b) / 2 + facing * 0.007 + UP * 0.003, b - span * 0.027], 0.0028, sides=5)
    head = objs[0]
    for obj in objs:
        T.sharpen(obj.data, 50)
    T.place(p, "goggles", "rims", metal, "steel", head)["hardware"] = "close"
    T.place(p, "goggles", "bridge", bm, "leather", head)


def detail(p, belt_z):
    gloves(p)
    belt(p, belt_z)
    pads(p)
    goggles(p)
