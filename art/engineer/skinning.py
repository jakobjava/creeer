"""The engineer's skinning (G1.3f; ADR-0026 decisions 6 and 7): the ragdoll's segments as an
armature, each piece's weights on them (`s.<segment>` attributes, from the body's bone heat,
`body.weigh_segments`), and those weights as at most four vertex groups a vertex for the export.
Each bone's head sits at its joint with its parent (the pelvis's at its centre), in the
ragdoll's frame, so a segment's pose moves its vertices.
"""

import json
import pathlib

import body as B
import bpy
from mathutils import Vector
from mathutils.kdtree import KDTree

HERE = pathlib.Path(__file__).resolve().parent
BONES = json.loads((HERE / "bones.json").read_text(encoding="utf-8"))
MOST_BONES = 4


def game_to_blender(p):
    return Vector((p[0], -p[2], p[1]))


def bone_places():
    """Each segment's bone (head, tail) in Blender's axes, in the ragdoll's frame."""
    centre = {s["name"]: game_to_blender(s["centre"]) for s in BONES["segments"]}
    at = {j["child"]: game_to_blender(j["at"]) for j in BONES["joints"]}
    out = {}
    for s in BONES["segments"]:
        name = s["name"]
        head = at.get(name, centre[name])
        children = [game_to_blender(j["at"]) for j in BONES["joints"] if j["parent"] == name and j["child"] in at]
        tail = min(children, key=lambda c: (c - head).length) if name not in ("pelvis", "torso") and children else 2 * centre[name] - head
        if name == "pelvis":
            tail = at["torso"]
        if name == "torso":
            tail = at["head"]
        out[name] = (head, tail)
    return out


def armature(scene):
    data = bpy.data.armatures.new("engineer")
    rig = bpy.data.objects.new("engineer", data)
    scene.collection.objects.link(rig)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.mode_set(mode="EDIT")
    for name, (head, tail) in bone_places().items():
        bone = data.edit_bones.new(name)
        bone.head, bone.tail = head, tail
    bpy.ops.object.mode_set(mode="OBJECT")
    return rig


def segment_weights(obj):
    """Each vertex's weights by segment, from the `s.<segment>` attributes."""
    out = {}
    for name in B.SEGMENTS:
        a = obj.data.attributes.get(f"s.{name}")
        if a is not None:
            values = [0.0] * len(obj.data.vertices)
            a.data.foreach_get("value", values)
            out[name] = values
    return out


def give_weights(obj, weights):
    """Writes `weights` (segment -> per vertex) as `obj`'s `s.<segment>` attributes."""
    for name in B.SEGMENTS:
        a = obj.data.attributes.get(f"s.{name}") or obj.data.attributes.new(f"s.{name}", "FLOAT", "POINT")
        a.data.foreach_set("value", weights.get(name, [0.0] * len(obj.data.vertices)))


def weigh_from(obj, skin, only):
    """`obj`'s vertices weighted as the skin's nearest vertex is (a collar, a belt), of the skin
    that weighs mostly on the segments `only`, and on those alone: from all of it, the belt took
    the forearms' (they hang as near as the hips are) and would have swung with the arms."""
    source = segment_weights(skin)
    tree = KDTree(len(skin.data.vertices))
    for v in skin.data.vertices:
        if sum(source[name][v.index] for name in only) > 0.5:
            tree.insert(v.co, v.index)
    tree.balance()
    nearest = [tree.find(v.co)[1] for v in obj.data.vertices]
    weights = {name: [source[name][i] for i in nearest] for name in only}
    totals = [sum(weights[name][k] for name in only) for k in range(len(nearest))]
    give_weights(obj, {name: [w / t for w, t in zip(ws, totals)] for name, ws in weights.items()})


def limb(segment):
    """The limb a segment belongs to (the trunk's: the pelvis, torso and head)."""
    if segment in ("pelvis", "torso", "head"):
        return "trunk"
    part, side = segment.split(".")
    return ("arm." if part in ("upper_arm", "forearm", "hand") else "leg.") + side


def transfer(objs, beneath, near=4, by_limb=True):
    """The garments `objs` weighted as what lies `beneath` them is (the skin, or the garment they
    are worn over): each point as its `near` nearest points of it (by inverse distance), given
    `by_limb` on the limb the point weighs most on (a hanging sleeve's inside by the arm, not
    the torso beside it; a layer next to the skin by the skin beneath it, which at the groin
    and the armpit is another limb's: by its own limb, the shorts came 9 mm short of the body
    at the hips' limits).
    Skinned with dual quaternions, a point moves as one beneath it with the same weights,
    whatever the distance between them, and the layers keep their order. Carried from the skin
    they were offset from, the weights had stayed behind where the cloth was spanned, smoothed
    and hung: the neckline took a shoulder's, and a raised arm drew it 12 mm into the neck.
    Each layer from the one beneath it: the jacket and the coverall from the skin each, where
    they hang apart they found it in different places, the coverall 12 mm through the jacket at
    the hips' limits; and from the skin where a ray in along its normal met it, worse still."""
    source = segment_weights(beneath)
    names = list(source)
    points = {}
    for v in beneath.data.vertices:
        points.setdefault(limb(max(names, key=lambda n: source[n][v.index])) if by_limb else "all", []).append(v)
    trees = {}
    for chain, vs in points.items():
        trees[chain] = KDTree(len(vs))
        for v in vs:
            trees[chain].insert(v.co, v.index)
        trees[chain].balance()
    for obj in objs:
        own = segment_weights(obj)
        out = {n: [0.0] * len(obj.data.vertices) for n in names}
        for v in obj.data.vertices:
            found = trees[limb(max(own, key=lambda n: own[n][v.index])) if by_limb else "all"].find_n(v.co, near)
            shares = [(i, 1.0 / max(d, 1e-5)) for _, i, d in found]
            total = sum(w for _, w in shares)
            for n in names:
                out[n][v.index] = sum(source[n][i] * w for i, w in shares) / total
        give_weights(obj, out)


# Where the boots bend (m below and above the ankle bone).
BEND = (0.01, 0.03)


def bends():
    """The planes bounding each boot's bend, for a level's faces to be cut along (`regions.cut`):
    a decimated shaft's long faces spanned the bend, and tilted with the foot (the coverall
    tucked inside came 6 mm through them at the ankle's limit)."""
    import outfit as O

    out = []
    for side in (1, -1):
        # Round the posed boot's ankle, at the height `stiff` bends it: about the fitted body's
        # (the feet apart before the legs are drawn in), the planes reached a few of its faces,
        # and 230 still spanned the bend.
        ankle = B.posed(B.joint("foot", side), side, "leg")
        ankle.z = B.joint("foot", side).z + O.SOLE
        out += [(ankle + Vector((0.0, 0.0, dz)), Vector((0.0, 0.0, 1.0)), 0.09) for dz in (-BEND[0], BEND[1])]
    return out


def stiff(boots, below=BEND[0], above=BEND[1]):
    """The boots' uppers weighted as stiff leather bends: each its foot's below its ankle bone
    and its shank's above, bending between `below` and `above` (m) about it. As the skin beneath,
    the shaft's long faces took the instep's foot weight 2.5 cm above the ankle, and tilting
    with the foot at the ankle's limit came 10 mm into the coverall tucked inside."""
    import outfit as O

    weights = {name: [0.0] * len(boots.data.vertices) for name in B.SEGMENTS}
    for v in boots.data.vertices:
        side, tag = (1, "L") if v.co.x > 0 else (-1, "R")
        ankle = B.joint("foot", side).z + O.SOLE
        t = min(max((v.co.z - ankle + below) / (above + below), 0.0), 1.0)
        shank = t * t * (3.0 - 2.0 * t)
        weights[f"shank.{tag}"][v.index] = shank
        weights[f"foot.{tag}"][v.index] = 1.0 - shank
    give_weights(boots, weights)


def weigh_rigidly(obj, segment):
    give_weights(obj, {segment: [1.0] * len(obj.data.vertices)})


def complete_weights(pieces, skin):
    """Weights for what was made apart from the skin: the eyes and goggles on the head, the soles
    on their feet; the garments as the skin beneath them (`transfer`), the collar as the skin
    under it, the belt as the jacket; and the trims (`detail.py`) as the garment they sit on
    (`host`) is, on its segments `only`, or on one segment (`rigid`)."""
    for obj in pieces["body"]:
        if obj.name.startswith("eye"):
            weigh_rigidly(obj, "head")
    for obj in pieces["goggles"]:
        weigh_rigidly(obj, "head")
    for obj in pieces["boots"]:
        if obj.name.startswith("sole"):
            side = "L" if sum(v.co.x for v in obj.data.vertices) > 0 else "R"
            weigh_rigidly(obj, f"foot.{side}")
    transfer([o for piece in ("base_layer", "gloves") for o in pieces[piece] if "s.pelvis" in o.data.attributes], skin, by_limb=False)
    transfer([o for o in pieces["coverall"] if "s.pelvis" in o.data.attributes], skin)
    # The coverall's sleeves end at the wrists, held by the forearms: as the skin beneath, a cuff
    # took the wrist's hand weight and turned with the hand, and the hand bent fully put the
    # glove's cuff 11.5 mm through it.
    for obj in pieces["coverall"]:
        if "s.pelvis" in obj.data.attributes:
            weights = segment_weights(obj)
            for tag in "LR":
                hand, forearm = weights[f"hand.{tag}"], weights[f"forearm.{tag}"]
                weights[f"forearm.{tag}"] = [f + h for f, h in zip(forearm, hand)]
                weights[f"hand.{tag}"] = [0.0] * len(hand)
            give_weights(obj, weights)
    coverall = next(o for o in pieces["coverall"] if o.name == "coverall")
    transfer([o for o in pieces["jacket"] if "s.pelvis" in o.data.attributes], coverall)
    # The knee pads rigid on the kneecaps, which turn half as far as the shins: as the coverall
    # beneath, they folded with the knee to its limit and came apart over it. The elbows'
    # patches as the jacket they are sewn on.
    for obj in pieces["knee_pads"]:
        if obj.name.startswith("knee pad"):
            tag = "L" if sum(v.co.x for v in obj.data.vertices) > 0 else "R"
            half = [0.5] * len(obj.data.vertices)
            give_weights(obj, {f"thigh.{tag}": half, f"shank.{tag}": half})
    jacket = next(o for o in pieces["jacket"] if o.name == "jacket")
    transfer([o for o in pieces["elbow_pads"] if o.name.startswith("elbow pad")], jacket)
    for obj in pieces["boots"]:
        if obj.name == "boots":
            stiff(obj)
    for obj in pieces["jacket"]:
        if "s.pelvis" not in obj.data.attributes and "host" not in obj:
            weigh_from(obj, skin, ("torso", "head"))
    # The belt as the jacket it is buckled over (its trunk and hips; not the sleeves beside it):
    # as the skin's pelvis and torso, it stayed put while the jacket's hips turned with the
    # thighs beneath it, 12 mm through it.
    for obj in pieces["belt"]:
        if "s.pelvis" not in obj.data.attributes and "host" not in obj:
            weigh_from(obj, jacket, ("pelvis", "torso", "thigh.L", "thigh.R"))
    for objs in pieces.values():
        for obj in objs:
            if "s.pelvis" in obj.data.attributes:
                continue
            if "rigid" in obj:
                weigh_rigidly(obj, obj["rigid"])
            elif "host" in obj:
                weigh_from(obj, bpy.data.objects[obj["host"]], tuple(obj.get("only", B.SEGMENTS)))


def skin_groups(obj, rig):
    """The segment weights as at most four vertex groups a vertex, summing to one, and the
    armature that moves them."""
    weights = segment_weights(obj)
    groups = {name: obj.vertex_groups.new(name=name) for name in B.SEGMENTS}
    for i in range(len(obj.data.vertices)):
        ranked = sorted(((w[i], n) for n, w in weights.items() if w[i] > 0.0), reverse=True)[:MOST_BONES]
        total = sum(w for w, _ in ranked)
        if total <= 0.0:
            raise ValueError(f"{obj.name}: vertex {i} has no weight")
        for w, n in ranked:
            groups[n].add([i], w / total, "REPLACE")
    for name in list(obj.data.attributes.keys()):
        if name.startswith(("s.", "w_")):
            obj.data.attributes.remove(obj.data.attributes[name])
    modifier = obj.modifiers.new("skin", "ARMATURE")
    modifier.object = rig
    obj.parent = rig
