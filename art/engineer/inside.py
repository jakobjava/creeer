"""What lies inside a wearable (G1.3f, ADR-0026 decision 7): the faces of a piece that a wearable
worn over it encloses take the region `in.<wearable>`, hidden while that wearable is worn
(`creeer_art::wearable`), as the body's regions under the garments are.

Skinning cannot part what a joint closed fully presses together: in the crook the skinning
itself folds over (the turn changing by more than a radian over the distance from the joint's
axis), and the layers there cross whatever their density; undecimated, the coverall came 11.9 mm
through the jacket at the elbow (ADR-0026, Measured and rejected). Hidden where it is enclosed,
the layer beneath is not drawn there to show through: the coverall's sleeves inside the
jacket's, its legs inside the boots, the gloves' cuffs inside the sleeves, the skin under the
vest and shorts.

A point lies inside a wearable when the wearable lies over it (within `NEAR` along the skin's
normal, the check's out, `check_fit.py`) and every line of sight from it within `CONE` of that
normal, from `BENEATH` under it (layers may touch at rest), meets the wearable as that level
draws it; a face when all its corners do. Along a point's own normal, tilted with a crease, a
line of sight went down a sleeve and out of it; asked at the face's middle, its corners nearer
the opening were seen; at the check's own 45°, some were seen at rest, and more as the joints
moved; at 70°, too little was hidden, and the elbow's crook came 11.7 mm through again.
"""

import math

import bpy
from mathutils import Matrix
from mathutils.bvhtree import BVHTree

# Each piece and the wearables worn over it that can hide it, enclosing it; those built first.
COVERS = {"body": ("base_layer",), "coverall": ("jacket", "boots"), "gloves": ("coverall",)}
FIRST = ("base_layer", "jacket", "boots", "coverall")
CONE, BENEATH, NEAR, REACH = math.radians(60.0), 0.003, 0.06, 0.5
# The regions never inside another piece.
OPEN = ("hair", "face", "head", "neck")


def tree(objs):
    """One tree of the faces of `objs` (meshes in the ragdoll's frame)."""
    verts, polys = [], []
    for obj in objs:
        base = len(verts)
        verts += [v.co.copy() for v in obj.data.vertices]
        polys += [tuple(base + i for i in p.vertices) for p in obj.data.polygons]
    return BVHTree.FromPolygons(verts, polys)


def order(pieces):
    """The pieces' names, each wearable that covers another before it."""
    return [p for p in FIRST if p in pieces] + [p for p in pieces if p not in FIRST]


class Covers:
    """The wearables that cover others at each level, as built (`add`, a level's parts but what
    lies inside another), and the skin, whose normals look out. Asked of the whole garments as
    built, the far level's coarser ones let hidden points be seen between their faces: 1,512
    over the poses and outfits, against 1,153."""

    def __init__(self, skin):
        self.skin = BVHTree.FromObject(skin, bpy.context.evaluated_depsgraph_get())
        self.trees = {}

    def add(self, piece, level, parts):
        if piece in FIRST:
            self.trees[(piece, level)] = tree([o for o in parts if ":in." not in o.name])

    def enclosed(self, wearable, level, p, out):
        """Whether `wearable` lies over `p` (looking out along `out`) and every line of sight
        from it within `CONE` of `out` meets it."""
        over = self.trees[(wearable, level)]
        if over.ray_cast(p + out * 1e-4, out, NEAR)[0] is None:
            return False
        t = out.orthogonal().normalized()
        ways = [out]
        for k in range(8):
            axis = Matrix.Rotation(k * math.pi / 4, 3, out) @ t
            ways += [Matrix.Rotation(a, 3, axis) @ out for a in (CONE / 2, CONE)]
        start = p - out * BENEATH
        return all(over.ray_cast(start, w, REACH)[0] is not None for w in ways)

    def relabel(self, obj, piece, level, regions):
        """`regions` (each face of `obj`'s, `piece` at `level`) with the faces inside a wearable
        over it given to that wearable."""
        vs = obj.data.vertices
        for wearable in COVERS.get(piece, ()):
            inside = {}

            def corner(i):
                if i not in inside:
                    v = vs[i]
                    out = v.normal if piece == "body" else self.skin.find_nearest(v.co)[1]
                    inside[i] = self.enclosed(wearable, level, v.co, out)
                return inside[i]

            for f in obj.data.polygons:
                if regions[f.index] not in OPEN and not regions[f.index].startswith("in.") and all(corner(i) for i in f.vertices):
                    regions[f.index] = "in." + wearable
        return regions
