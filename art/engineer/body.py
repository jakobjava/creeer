"""The engineer's body (G1.3f; ADR-0026): Blender Studio's realistic man (Human Base Meshes
v1.4.1, CC0; `art/base_meshes.py` fetches it) fitted to ANSUR II's men at their means scaled to
1.80 m, then posed from his A-pose to stand with the arms hanging and the feet under the hips.

The fit: heights are remapped through measured landmarks (his crotch is 4.7 cm low, his trunk
3 cm long), the arms are lengthened along their axes to ANSUR's wrist height, and everything is
scaled across by the head's scale (his head and neck already match within a centimetre). The
limbs move by blended rotations about their joints, weighted by Blender's bone heat: a skin of
one bone per limb, applied once. Garments made as offsets of the fitted body carry the same
weights, so they pose with it.
"""

import math
import pathlib

import bpy
from mathutils import Matrix, Vector

try:
    _ART = pathlib.Path(__file__).resolve().parent.parent
except NameError:  # run by exec() in a live session
    _ART = pathlib.Path(r"C:\Users\jakmo\dev\creëer\art")

BODY = "GEO-body_male_realistic"
EYES = ("GEO-body_male_realistic.eye.L", "GEO-body_male_realistic.eye.R")
LEVEL = 1  # of his three multires levels: 42k faces, the soft forms of art-style.md §8

# Measured on his level-1 surface, soles at 0, in his units (m; Blender's axes, facing −y), by
# sections through it: where the legs part, the notch above the sternum on the midline profile,
# the top of the head, the outer ankle bone, and the joints along the limbs' sections.
TOP = 1.6899
STERNUM = 1.37
CROTCH = 0.77
ANKLE = 0.07
SHOULDER = Vector((0.175, 0.005, 1.34))
ELBOW = Vector((0.297, -0.002, 1.087))
WRIST = Vector((0.376, -0.068, 0.876))
FINGERTIP = Vector((0.43, -0.14, 0.69))
HIP = Vector((0.08, 0.0, 0.83))
KNEE = Vector((0.149, 0.036, 0.44))
FOOT = Vector((0.178, 0.05, 0.075))
TOE = Vector((0.21, -0.1, 0.02))

# ANSUR II's men's means at 1.80 m: stature, crotch height, lateral malleolus height, wrist
# height. The head and neck keep their shape: scaled by K, the top at the stature.
STATURE = 1.80
K = STATURE / TOP
KNOTS = [(0.0, 0.0), (ANKLE, 0.075), (CROTCH, 0.867), (STERNUM, STATURE - K * (TOP - STERNUM)), (TOP, STATURE)]
WRIST_HEIGHT = 0.869
# The pose: the arms lowered from the A-pose until the upper arms hang by the trunk, the
# forearms in line with them, palms to the thighs (the carrying angle, 11.0° in men by
# Paraskevas et al. 2004, shows with the palms forward; turned to the thighs, the forearm
# lines up with the upper arm); the legs drawn in until the ankles are 11 cm from the middle.
LOWER_ARMS = math.radians(19.0)
CARRYING_ANGLE = 0.0
DRAW_IN_LEGS = math.radians(4.5)


def _frontal(v):
    """How far `v` (pointing down a limb on the +x side) leans out from straight down."""
    return math.atan2(v.x, -v.z)


# His A-pose's forearm leans in from his upper arm's line; turned out by this, it makes
# `CARRYING_ANGLE` with it.
CARRY = CARRYING_ANGLE - (_frontal(WRIST - ELBOW) - _frontal(ELBOW - SHOULDER))

# The skeleton's bones along each limb, and the weights kept: the feet apart from the legs, to
# stay flat on the ground when the legs turn.
CHAINS = {"arm": ("upper_arm", "forearm", "hand"), "leg": ("thigh", "shank", "foot")}
GROUPS = {
    "arm": ("upper_arm", "forearm", "hand"),
    "forearm": ("forearm", "hand"),
    "leg": ("thigh", "shank"),
    "foot": ("foot",),
}


def _slopes(knots):
    """Fritsch–Carlson slopes: a monotone cubic through the knots, with no overshoot."""
    secants = [(b[1] - a[1]) / (b[0] - a[0]) for a, b in zip(knots, knots[1:])]
    out = [secants[0]]
    for s, t in zip(secants, secants[1:]):
        out.append(0.0 if s * t <= 0 else 2.0 / (1.0 / s + 1.0 / t))
    return out + [secants[-1]]


SLOPES = _slopes(KNOTS)


def height(z):
    """A height of his (m) as the fitted body's."""
    if z >= KNOTS[-1][0]:
        return KNOTS[-1][1] + K * (z - KNOTS[-1][0])
    if z <= 0:
        return KNOTS[1][1] / KNOTS[1][0] * z
    k = next(i for i in range(len(KNOTS) - 1) if z <= KNOTS[i + 1][0])
    (x0, y0), (x1, y1) = KNOTS[k], KNOTS[k + 1]
    h, t = x1 - x0, (z - x0) / (x1 - x0)
    return (
        (2 * t**3 - 3 * t**2 + 1) * y0
        + (t**3 - 2 * t**2 + t) * h * SLOPES[k]
        + (-2 * t**3 + 3 * t**2) * y1
        + (t**3 - t**2) * h * SLOPES[k + 1]
    )


def mirrored(v, side):
    return Vector((v.x * side, v.y, v.z))


def trunk(p):
    return Vector((K * p.x, K * p.y, height(p.z)))


def _arm_scale(side):
    """How much the arm is lengthened along its axis for the hanging wrist to be at ANSUR's
    height."""
    s, e, w = (mirrored(v, side) for v in (SHOULDER, ELBOW, WRIST))
    drop = (_turn(side * LOWER_ARMS) @ ((e - s) + _turn(-side * CARRY) @ (w - e))).z
    return (trunk(s).z - WRIST_HEIGHT) / (-drop * K)


def _turn(angle):
    """A rotation in the frontal plane (about y): positive moves +x below a joint inward."""
    return Matrix.Rotation(angle, 3, "Y")


def _arm(p, side):
    """A point of the arm on `side` fitted (A-pose): the shoulder where the trunk's fit puts it,
    the arm lengthened along the shoulder–wrist axis."""
    s = mirrored(SHOULDER, side)
    axis = (mirrored(WRIST, side) - s).normalized()
    q = p - s
    q += (ARM_SCALE[side] - 1.0) * q.dot(axis) * axis
    return trunk(s) + K * q


ARM_SCALE = {}
ARM_SCALE.update({side: _arm_scale(side) for side in (-1, 1)})


JOINTS = {
    "shoulder": SHOULDER,
    "elbow": ELBOW,
    "wrist": WRIST,
    "fingertip": FINGERTIP,
    "hip": HIP,
    "knee": KNEE,
    "foot": FOOT,
    "toe": TOE,
}


# How far the arms are moved in towards the trunk after the fit (m; set by `physique`).
ARM_IN = 0.0


def joint(name, side):
    """A joint of his (on `side`: +1 his left, at +x) where the fitted body has it, before the
    pose."""
    p = mirrored(JOINTS[name], side)
    if name in ("shoulder", "elbow", "wrist", "fingertip"):
        return _arm(p, side) - Vector((side * ARM_IN, 0.0, 0.0))
    return trunk(p)


def load(scene):
    """His body and eyes, appended from the bundle into `scene`: the multires level baked in,
    the objects' transforms baked into their meshes, the soles at 0 and his middle at x = 0.
    Only these objects' data is taken from the file; nothing in it runs."""
    import sys

    sys.path.insert(0, str(_ART))
    import base_meshes

    with bpy.data.libraries.load(str(base_meshes.blend()), link=False) as (src, dst):
        dst.objects = [BODY, *EYES]
    objs = list(dst.objects)
    for obj in objs:
        scene.collection.objects.link(obj)
        assert obj.animation_data is None and obj.data.animation_data is None
    body = objs[0]
    multires = next(m for m in body.modifiers if m.type == "MULTIRES")
    multires.levels = LEVEL
    with bpy.context.temp_override(object=body, active_object=body):
        bpy.ops.object.modifier_apply(modifier=multires.name)
    lo = min((body.matrix_world @ v.co).z for v in body.data.vertices)
    shift = Matrix.Translation((-body.matrix_world.translation.x, 0.0, -lo))
    for obj in objs:
        world = shift @ obj.matrix_world
        obj.parent = None
        obj.data.transform(world)
        obj.matrix_world = Matrix.Identity(4)
    for library in list(bpy.data.libraries):
        if library.filepath == str(base_meshes.blend()) and not library.users_id:
            bpy.data.libraries.remove(library)
    return body, objs[1:]


def weigh(body):
    """Bone heat weights of a one-bone-per-limb skeleton at his joints, kept on `body` as one
    attribute per limb (`w_arm.L`, …) summed over the limb's bones."""
    data = bpy.data.armatures.new("fit skeleton")
    rig = bpy.data.objects.new("fit skeleton", data)
    body.users_scene[0].collection.objects.link(rig)
    bones = {"trunk": (Vector((0, 0, HIP.z)), Vector((0, 0, STERNUM))), "head": (Vector((0, 0, STERNUM)), Vector((0, 0, TOP)))}
    for side, tag in ((1, "L"), (-1, "R")):
        m = lambda v: mirrored(v, side)  # noqa: E731
        chain = {"arm": (SHOULDER, ELBOW, WRIST, FINGERTIP), "leg": (HIP, KNEE, FOOT, TOE)}
        for limb, points in chain.items():
            for name, a, b in zip(CHAINS[limb], points, points[1:]):
                bones[f"{name}.{tag}"] = (m(a), m(b))
    # Edit mode and parenting work on the active scene's view layer.
    layer = bpy.context.view_layer
    layer.objects.active = rig
    bpy.ops.object.mode_set(mode="EDIT")
    for name, (head, tail) in bones.items():
        bone = data.edit_bones.new(name)
        bone.head, bone.tail = head, tail
    bpy.ops.object.mode_set(mode="OBJECT")
    for obj in layer.objects:
        obj.select_set(obj in (body, rig))
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")
    for limb, names in GROUPS.items():
        for tag in "LR":
            groups = [body.vertex_groups[f"{n}.{tag}"].index for n in names]
            attr = body.data.attributes.new(f"w_{limb}.{tag}", "FLOAT", "POINT")
            for v in body.data.vertices:
                total = sum(g.weight for g in v.groups)
                mine = sum(g.weight for g in v.groups if g.group in groups)
                attr.data[v.index].value = mine / total if total > 0 else 0.0
    body.parent = None
    body.modifiers.clear()
    body.vertex_groups.clear()
    bpy.data.objects.remove(rig)
    bpy.data.armatures.remove(data)


def weights(mesh):
    """The limbs' weights at each vertex of `mesh` (zero where it has none)."""
    out = {}
    for limb in GROUPS:
        for side, tag in ((1, "L"), (-1, "R")):
            a = mesh.attributes.get(f"w_{limb}.{tag}")
            out[(limb, side)] = [d.value for d in a.data] if a else [0.0] * len(mesh.vertices)
    return out


def fit(obj):
    """Fits `obj`'s vertices (his units, A-pose) to the person (m, A-pose)."""
    w = weights(obj.data)
    for i, v in enumerate(obj.data.vertices):
        p = v.co.copy()
        arm = {side: w[("arm", side)][i] for side in (-1, 1)}
        out = trunk(p) * (1.0 - sum(arm.values()))
        for side, a in arm.items():
            if a:
                out += a * _arm(p, side)
        v.co = out
    obj.data.update()


def posed(p, side, limb):
    """Where a point fully on `limb` (`side`) ends up after the pose: the arms lowered about
    the shoulders, the legs drawn in about the hips, the feet carried along flat."""
    if limb == "foot":
        # Carried across with the ankle but not down: the soles stay on the ground.
        ankle = joint("foot", side)
        moved = posed(ankle, side, "leg") - ankle
        return p + Vector((moved.x, moved.y, 0.0))
    if limb == "forearm":
        elbow = joint("elbow", side)
        return posed(elbow + _turn(-side * CARRY) @ (p - elbow), side, "arm")
    at = joint("shoulder" if limb == "arm" else "hip", side)
    angle = LOWER_ARMS if limb == "arm" else DRAW_IN_LEGS
    return at + _turn(side * angle) @ (p - at)


def pose(obj):
    """Poses `obj` (fitted, A-pose), blending each limb's motion by its weights (the forearm's
    on top of the whole arm's, which includes it)."""
    w = weights(obj.data)
    for i, v in enumerate(obj.data.vertices):
        p = v.co.copy()
        out = p.copy()
        for (limb, side), a in w.items():
            if a[i]:
                start = posed(p, side, "arm") if limb == "forearm" else p
                out += a[i] * (posed(p, side, limb) - start)
        v.co = out
    obj.data.update()
