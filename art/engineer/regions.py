"""The engineer's regions (G1.3f, ADR-0026 decision 7): each face of a piece given to a region (one
of `creeer_art::model_skin::REGIONS`), so a worn wearable leaves out what it hides
(`data/wearables.ron`), and the piece divided into one object a region.

Each face goes to the segment its corners weigh most on. The head is split into face, neck and
head, the hair and the eyes are their own. The body's hidden regions end inside the garments'
openings (`outfit.openings`): the skin within the collar is the neck's, within the sleeves' ends
the hands', within the trouser legs' the feet's. Hidden, it showed the collar's inside through
the throat (`check_fit.py`); a hidden region lies wholly inside what hides it."""

import bmesh
import skinning as S
from mathutils import Vector

# The head's division (m, the ragdoll's frame): the neck below the plane through the chin and
# the nape; the face in front of the ears and below the hairline.
JAW = ((0.0, -0.09, 1.585), (0.0, 0.07, 1.66))
FACE_FRONT, FACE_TOP = -0.06, 1.765
# The body's parts the openings divide (`outfit.openings`): which segment's faces each takes
# (the head's below the jaw are the torso's by then), and how near its point (m) they lie.
PARTED = {"neck": (("torso",), 0.25), "hand": (("forearm",), 0.08), "foot": (("shank",), 0.16)}


def below(co, plane):
    front, back = (Vector(p) for p in plane)
    d = back - front
    n = Vector((0.0, -d.z, d.y)).normalized()
    if n.z < 0:
        n = -n
    return (co - front).dot(n) < 0


def beyond(c, segment, openings):
    """The region past an opening that the body's face at `c` on `segment` lies in, or None."""
    for name, (co, no) in openings.items():
        kind = name.split(".")[0]
        segments, reach = PARTED[kind]
        if segment.split(".")[0] in segments and (c - co).length < reach and (c - co).dot(no) > 0:
            return name
    return None


def face_regions(obj, piece, openings):
    """Each face's region. `openings` (`outfit.openings`) part the body's."""
    weights = S.segment_weights(obj)
    names = list(weights)
    materials = [m.name if m else "" for m in obj.data.materials]
    out = []
    for f in obj.data.polygons:
        material = materials[f.material_index] if materials else ""
        c = f.center
        if material == "hair":
            # The brows are the face's (a helmet hiding the hair leaves them).
            out.append("face" if piece == "body" and c.y < FACE_FRONT and c.z < FACE_TOP - 0.02 else "hair")
            continue
        if material in ("eye_white", "iris"):
            out.append("face")
            continue
        totals = [sum(weights[n][v] for v in f.vertices) for n in names]
        region = names[max(range(len(names)), key=totals.__getitem__)]
        if region == "head" and below(c, JAW):
            region = "neck" if piece != "body" else "torso"
        elif region == "head" and piece == "body" and c.y < FACE_FRONT and c.z < FACE_TOP:
            region = "face"
        if piece == "body":
            region = beyond(c, region, openings) or region
        out.append(region)
    return out


def cut_openings(obj, openings):
    """The body cut along each opening's plane, so its drawn region ends in a clean line inside
    the garment, not in the faces' saw-teeth."""
    cut(obj, [(co, no, PARTED[name.split(".")[0]][1]) for name, (co, no) in openings.items()])


def cut(obj, planes):
    """`obj` cut along each plane (a point, its normal, and how near the point it is cut, m)."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    for co, no, reach in planes:
        # Every face across the plane near its point: one left whole, its middle on one side,
        # carried a corner 4 cm past it into the other region.
        near = [f for f in bm.faces if any((v.co - co).length < reach for v in f.verts) and min((v.co - co).dot(no) for v in f.verts) < 0 < max((v.co - co).dot(no) for v in f.verts)]
        geom = list({v for f in near for v in f.verts}) + list({e for f in near for e in f.edges}) + near
        bmesh.ops.bisect_plane(bm, geom=geom, plane_co=co, plane_no=no, dist=1e-5)
    bm.to_mesh(obj.data)
    bm.free()


def split(scene, obj, piece, level, regions, rig):
    """`obj` divided into one object per region, `<piece>:<region>@<level>`, skinned."""
    out = []
    for region in sorted(set(regions)):
        part = obj.copy()
        part.data = obj.data.copy()
        scene.collection.objects.link(part)
        bm = bmesh.new()
        bm.from_mesh(part.data)
        bm.faces.ensure_lookup_table()
        bmesh.ops.delete(bm, geom=[f for f in bm.faces if regions[f.index] != region], context="FACES")
        bm.to_mesh(part.data)
        bm.free()
        part.name = part.data.name = f"{piece}:{region}@{level}"
        S.skin_groups(part, rig)
        out.append(part)
    return out
