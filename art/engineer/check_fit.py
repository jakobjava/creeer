"""The engineer's fit (G1.3f validation, `docs/g1-plan.md`): on the exported model's close-up
level, posed by its skin at rest and at each of the ragdoll's joint limits, for the base body
alone, the full outfit and each wearable alone:

- **no poke-through**: no point of a layer beneath shows through a layer over it by more than
  2 mm. From each point of the layer beneath, a ray back along its normal finds the layer over
  it within 1.2 cm only where that layer has sunk below it, on the same limb (or the trunk):
  further, or on another limb, it found a neighbouring part (a hand beside the thigh, an arm
  swung into the hip), not a layer over it. A strapped-on layer is over its limbs alone (`ON`):
  the hanging sleeve resting against the belt is not under it. And only a point beneath the
  layer at rest (a ray out along its normal met it within 10 cm) can come through it, and only
  through the part of it that lay over it (within 3 cm of where that ray met it): parts that
  meet only as a joint closes on them (the forearm's sleeve folded onto the upper arm's in the
  crook of the elbow, the gloved hand bent into the cuff) are in contact, which skinning cannot
  part, and are printed apart, not failed;
- **no holes**: no point of a region a worn layer hides (`data/wearables.ron`) is seen: no
  line of sight from it gets out past everything the outfit draws (cloth, or the skin it leaves
  bare: drawn two-sided, any face stops it), along its outward normal or 45° off it four ways,
  each confirmed by rays 3° either side (one ray alone slipped between two faces along a
  seam). Hidden skin is not drawn, and stops nothing. Out is the body's normal, for a garment's
  point too (the body's nearest face's): a garment's rims and inner faces face along it or into
  it, and are covered with its outer face. Asking for cloth within a reach on the same limb
  failed where cloth hangs 8 cm off the shank, or a ray from the armpit crossed the hidden
  torso; asking that the first face be met from inside failed where a sleeve lowered from the
  A-pose passes into the garment's torso, or rests against the hip. A hidden point that has
  come through what hides it (within 1.2 cm of it, on its outer side) is not drawn, and shows
  nothing;
- **within its segments** (at rest): each point lies within the box (the ragdoll's) of a segment
  it is weighted to by a fifth or more, grown by 3 cm for the body and 5 cm for the outfit; the
  shares outside are printed by the point's main segment, as the blockout's were
  (`docs/g1-plan.md`). By its main segment's box alone, a point by a joint (the jacket's hem,
  on the thigh's bone but beside the pelvis) read as outside.

At a coarser level each piece is allowed its own error against the close-up level (the 99th
percentile of the distance between them, at rest): the game draws a level only where its error
projects under half a pixel (ADR-0026's levels), so a miss within it is under half a pixel too.
A hidden point may come through what hides it by three times that: the far boots, a few hundred
triangles, leave the toes 2 cm outside them, not drawn, the boots whole over them.

Skinned with dual quaternions, as the game skins (ADR-0026 decision 6; Blender's "preserve
volume"): linear blending collapses the armpit and the inner elbow at the limits. Only points
drawn count: a layer hidden whole under another (the base layer under the coverall) shows
nowhere, whatever it pokes through. What lies inside a worn wearable (a piece's part
`in.<wearable>`, `inside.py`) is hidden as the regions it lists are, each point judged on its
segment's limb. A point's normal is the whole piece's: at a part's edge its own leaned off it.

Joint limits as `bones.json` gives them (the ragdoll's): a hinge at each end of its range (the
child turned by minus the angle about the axis: the elbow then bends forward, the knee back), a
ball bent to its cone in four directions. Each joint is posed alone, the bones below it turning
with it, as far as the ragdoll reaches: an arm stops where its box meets the pelvis's, torso's or
head's (`reach.py`). Run

    blender --background --factory-startup --python art/engineer/check_fit.py -- [<level>]

after `cargo xtask art --blend engineer`. It prints each pose's misses per outfit and exits 1 if
there are any.
"""

import json
import math
import pathlib
import sys

import bpy
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import fit_report as F  # noqa: E402  (after the path is set)
import reach  # noqa: E402
import wearables as W  # noqa: E402

ROOT = HERE.parent.parent
BONES = json.loads((HERE / "bones.json").read_text(encoding="utf-8"))
# What each layer is worn over. The gloves' cuffs are tucked under the coverall's sleeves; its
# legs into the boots.
OVER = {
    "base_layer": ("body",),
    "coverall": ("body", "base_layer", "gloves"),
    "jacket": ("body", "base_layer", "coverall"),
    "gloves": ("body",),
    "boots": ("body", "coverall"),
    "belt": ("coverall", "jacket"),
    "knee_pads": ("coverall",),
    "elbow_pads": ("jacket",),
    "goggles": ("body",),
}
# The limbs a layer is worn over, where not all: the belt is over the trunk and the hips, not the
# sleeve hanging against it (an arm resting on the trunk, as the ragdoll's arms do).
ON = {"belt": ("trunk", "leg"), "knee_pads": ("leg",), "elbow_pads": ("arm",), "gloves": ("arm",), "boots": ("leg",), "goggles": ("head",)}
POKE, REACH, SEEN, BENEATH, OVERLAID = 0.002, 0.012, 2.0, 0.1, 0.03
# The limbs: a region's chain. Points are compared within a chain, or with the trunk's.
CHAINS = {"head": "head", "face": "head", "hair": "head", "neck": "trunk", "torso": "trunk", "pelvis": "trunk"}


def part(region):
    """The part a point's `region` names (an `in.<wearable>` part's, without its segment)."""
    return region.split("/")[0]


def chain(region):
    region = region.split("/")[-1]
    return CHAINS.get(region) or region.replace("upper_arm", "arm").replace("forearm", "arm").replace("hand", "arm").replace("thigh", "leg").replace("shank", "leg").replace("foot", "leg")


def near(a, b):
    return a == b or "trunk" in (a, b)


def g2b(p):
    return Vector((p[0], -p[2], p[1]))


def poses():
    """(name, joint, axis in Blender's axes, angle) at rest and at every limit."""
    yield "rest", None, None, 0.0
    for j in BONES["joints"]:
        if j["kind"] == "hinge":
            for angle in j["range"]:
                if abs(angle) > 1e-6:
                    yield f"{j['name']} {angle:+.1f}", j, g2b(j["axis"]), -angle
        else:
            for axis in ((1, 0, 0), (-1, 0, 0), (0, 0, 1), (0, 0, -1)):
                yield f"{j['name']} cone {axis}", j, g2b(axis), j["cone"]


def reached(rig, joint, axis, angle):
    """The share of `angle` the ragdoll reaches at `joint` (`reach.py`), posed there: all of it,
    or as far as an arm goes before its box meets the core's."""
    pose(rig, joint, axis, angle)
    if joint is None or not reach.meets(rig):
        return 1.0
    lo, hi = 0.0, 1.0
    for _ in range(12):
        mid = 0.5 * (lo + hi)
        pose(rig, joint, axis, mid * angle)
        lo, hi = (mid, hi) if not reach.meets(rig) else (lo, mid)
    pose(rig, joint, axis, lo * angle)
    return lo


def below(name):
    """Segment `name` and every segment below it."""
    out = [name]
    for j in BONES["joints"]:
        if j["parent"] == name:
            out += below(j["child"])
    return out


def pose(rig, joint, axis, angle):
    for pb in rig.pose.bones:
        pb.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()
    if joint is None:
        return
    pivot = g2b(joint["at"])
    m = Matrix.Translation(pivot) @ Matrix.Rotation(angle, 4, axis) @ Matrix.Translation(-pivot)
    for name in below(joint["child"]):
        pb = rig.pose.bones[name]
        pb.matrix = m @ pb.bone.matrix_local
        bpy.context.view_layer.update()


def surfaces(objects):
    """Each piece's posed surface: its points, normals, regions, a tree to cast at, its faces'
    regions and its faces."""
    graph = bpy.context.evaluated_depsgraph_get()
    out = {}
    for piece, objs in objects.items():
        points, normals, regions, verts, polys, faces = [], [], [], [], [], []
        for obj in objs:
            mesh = obj.evaluated_get(graph).to_mesh()
            mesh.transform(obj.matrix_world)
            base = len(verts)
            region = obj.name.split(":")[1].split("@")[0]
            # A part inside a wearable (`inside.py`) keeps each point's segment, for its limb.
            names = {g.index: g.name for g in obj.vertex_groups}
            for v, w in zip(mesh.vertices, obj.data.vertices):
                verts.append(v.co.copy())
                points.append(v.co.copy())
                normals.append(v.normal.copy())
                regions.append(f"{region}/{names[max(w.groups, key=lambda g: g.weight).group]}" if region.startswith("in.") else region)
            polys += [tuple(base + i for i in p.vertices) for p in mesh.polygons]
            faces += [regions[base + p.vertices[0]] for p in mesh.polygons]
            obj.evaluated_get(graph).to_mesh_clear()
        # Normals as the whole piece's: at a part's edge its own leaned off them.
        total = {}
        for p, n in zip(points, normals):
            total[p.to_tuple(5)] = total.get(p.to_tuple(5), Vector()) + n
        normals = [total[p.to_tuple(5)].normalized() for p in points]
        out[piece] = (points, normals, regions, BVHTree.FromPolygons(verts, polys), faces, polys)
    return out


def drawn(surfs, worn, hidden):
    """What the outfit `worn` draws: a tree of its pieces' faces but the hidden regions', and
    each face's region."""
    verts, polys, regions = [], [], []
    for piece in worn:
        points, faces, own = surfs[piece][0], surfs[piece][5], surfs[piece][4]
        base = len(verts)
        verts += points
        gone = hidden.get(piece, ())
        for face, region in zip(own, faces):
            if part(face) not in gone:
                polys.append(tuple(base + i for i in region))
                regions.append(face)
    return BVHTree.FromPolygons(verts, polys), regions


def beneath(surfs):
    """For each layer and one worn over it, where (at rest, `surfs`) each of its points lies
    beneath it: the point a ray out along its normal meets it, or None."""
    out = {"rest": {piece: s[0] for piece, s in surfs.items()}}
    for outer, inners in OVER.items():
        for inner in inners:
            if outer in surfs and inner in surfs:
                tree = surfs[outer][3]
                out[(outer, inner)] = [tree.ray_cast(p + n * 1e-4, n, BENEATH)[0] for p, n in zip(*surfs[inner][:2])]
    return out


def overlaid(under, outer, face, polys, at):
    """Whether the posed face `face` of `outer` (its `polys`) lay over the rest point `at` (None:
    nothing did), within `OVERLAID`."""
    if at is None:
        return False
    rest = under["rest"][outer]
    corners = polys[face]
    return (sum((rest[i] for i in corners), Vector()) / len(corners) - at).length < OVERLAID


def errors(objects, level):
    """Each piece's error at `level` against the close-up level (m, at rest): the 99th
    percentile of the distance from each to the other's surface."""
    if level == 0:
        return {piece: 0.0 for piece in objects}
    out = {}
    for piece, objs in objects.items():
        close = [bpy.data.objects[o.name.replace(f"@{level}", "@0")] for o in objs]
        a, b = surfaces({0: close, 1: objs}).values()
        d = sorted([b[3].find_nearest(p)[3] for p in a[0]] + [a[3].find_nearest(p)[3] for p in b[0]])
        out[piece] = d[int(0.99 * (len(d) - 1))]
    return out


def misses(surfs, worn, hidden, under, slack):
    """(poke-throughs, holes, contacts) of the outfit `worn` in the posed `surfs`; `under`, which
    points lay beneath which layers at rest (`beneath`); `slack`, each piece's level's error."""
    pokes, holes, contacts = [], [], []
    for outer in worn:
        tree, faces, polys = surfs[outer][3], surfs[outer][4], surfs[outer][5]
        for inner in OVER.get(outer, ()):
            if inner not in worn:
                continue
            points, normals, regions = surfs[inner][:3]
            on = ON.get(outer)
            for p, n, r, below in zip(points, normals, regions, under[(outer, inner)]):
                if part(r) in hidden.get(inner, ()) or (on and not chain(r).startswith(on)):
                    continue
                hit, normal, face, depth = tree.ray_cast(p + n * 1e-4, -n, REACH)
                if hit is not None and normal.dot(n) > 0.0 and depth > max(POKE, slack[outer], slack[inner]) and near(chain(r), chain(faces[face])):
                    layered = overlaid(under, outer, face, polys, below)
                    (pokes if layered else contacts).append((depth, outer, inner, tuple(round(c, 3) for c in p)))
    body = surfs["body"][3]
    tree, faces = drawn(surfs, worn, hidden)
    for piece, regions in hidden.items():
        if piece not in worn:
            continue
        points, normals, own = surfs[piece][:3]
        overs = [o for o in worn if piece in OVER.get(o, ())]
        for p, n, r in zip(points, normals, own):
            if part(r) not in regions:
                continue
            out = n if piece == "body" else body.find_nearest(p)[1]
            if seen(tree, p, out) and not any(through(surfs[o], p, max(REACH, 3 * slack[o], 3 * slack[piece])) for o in overs):
                holes.append((piece, r, tuple(round(c, 3) for c in p)))
    return pokes, holes, contacts


def ways(out, angle):
    """Four directions `angle` (radians) off `out`, about two axes across it."""
    t = out.orthogonal().normalized()
    return [Matrix.Rotation(sign * angle, 3, axis) @ out for axis in (t, out.cross(t)) for sign in (1, -1)]


def seen(tree, p, out):
    """Whether a line of sight from `p` gets out past what `tree` draws."""
    gets_out = lambda way: tree.ray_cast(p + way * 1e-4, way, SEEN)[0] is None  # noqa: E731
    return any(gets_out(way) and all(gets_out(w) for w in ways(way, math.radians(3))) for way in (out, *ways(out, math.radians(45))))


def through(surf, p, reach=REACH):
    """Whether `p` has come through `surf`: within `reach` of it, on its outer side."""
    q, normal, _, distance = surf[3].find_nearest(p, reach)
    return q is not None and (p - q).dot(normal) > 0.0








def outfits(pieces):
    """The base body (in its base layer), the full outfit, and each wearable alone on the base
    body."""
    wearables = [p for p in pieces if p != "body"]
    yield "base body", {"body", "base_layer"}
    yield "full outfit", {"body", *wearables}
    for w in wearables:
        if w != "base_layer":
            yield f"{w} alone", {"body", "base_layer", w}


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    level = int(args[0]) if args else 0
    bpy.ops.wm.open_mainfile(filepath=str(ROOT / "target" / "art" / "engineer.blend"))
    rig = bpy.data.objects["engineer"]
    objects = {}
    for obj in bpy.data.objects:
        if obj.type == "MESH" and obj.name.endswith(f"@{level}"):
            objects.setdefault(obj.name.split(":")[0], []).append(obj)
            for mod in obj.modifiers:
                if mod.type == "ARMATURE":
                    mod.use_deform_preserve_volume = True
    covering = W.hides()
    failed = False
    pose(rig, None, None, 0.0)
    under = beneath(surfaces(objects))
    slack = errors(objects, level)
    if level:
        print("art: fit: errors against the close-up level (mm): " + ", ".join(f"{p} {1000 * e:.1f}" for p, e in sorted(slack.items())))
    for piece, shares in F.boxes(objects).items():
        for seg, share in sorted(shares.items()):
            print(f"art: fit: {piece} outside {seg}'s box: {100 * share:.1f} %")
    for name, joint, axis, angle in poses():
        share = reached(rig, joint, axis, angle)
        if share < 1.0:
            name += f" (to {share * abs(angle):.2f}: the arm meets the trunk)"
        surfs = surfaces(objects)
        for outfit, worn in outfits(objects):
            hidden = W.hidden(worn, covering, objects)
            pokes, holes, contacts = misses(surfs, worn, hidden, under, slack)
            if contacts:
                print(f"art: fit: {name}: {outfit}: {len(contacts)} in contact{F.through_by(contacts, 'against')}")
            if pokes or holes:
                failed = True
                print(f"art: fit: {name}: {outfit}: {len(pokes)} poking through{F.through_by(pokes)}; {len(holes)} uncovered{F.gaps(holes)}")
        print(f"art: fit: {name} checked")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    try:
        main()
    except SystemExit:
        raise
    except Exception:  # Blender would otherwise keep running with exit code 0
        import traceback

        traceback.print_exc()
        sys.exit(1)
