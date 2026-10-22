"""What a model carries for the renderer's surfaces (docs/art-style.md §6, ADR-0026), baked per
face corner once its normals are set and before its faces are triangulated:

- the "Color" attribute: red = ambient occlusion, green = edge wear, blue = the part's tone,
  alpha = the height above the model's base (for the ground's splash and dirt);
- the "UVMap": grain coordinates in metres, along each part's grain (u) and across it (v), offset
  per part so no two parts share their grain.

Parts are tagged when they are made (common.box): "part" (an id) and "grain" (the axis, 0-2, its
grain runs along) on every face.
"""

import math

import bmesh
from mathutils import Vector
from mathutils.bvhtree import BVHTree

# Ambient occlusion: rays per corner, and how far an occluder counts (m). Occlusion between
# parts and from the ground is the renderer's (SSAO, G1.3d); the bake adds the small creases
# screen space resolves poorly at a distance.
AO_RAYS = 96
AO_DISTANCE = 0.08
# A face larger than this (m²) takes one occlusion, the mean of its corners'.
UNIFORM_AREA = 0.0025
# Heights above a model's base are stored over this range (m); taller parts read 1.
HEIGHT_RANGE = 2.0
# Edges whose faces meet at more than this are sharp (shaded hard), in radians.
SHARP = math.radians(30)
# A sample point with another part this close above it (m) is covered by that part.
COVERED = 0.01
# The widest face that counts as a bevel (art-style.md §7: bevels are at most 3 cm), in metres.
BEVEL_WIDTH = 0.035
# How high the ground's splash and dirt reach up a model standing on it (m): the renderer's
# and the preview's, from the stored height.
SPLASH = 0.35


def hash01(n, salt):
    """A fixed pseudo-random number in [0, 1) for integer `n` (Python's hash() is not fixed)."""
    x = (n * 2654435761 + salt * 40503 + 0x9E3779B9) & 0xFFFFFFFF
    x ^= x >> 16
    x = (x * 0x45D9F3B) & 0xFFFFFFFF
    x ^= x >> 16
    return x / 2.0**32


def _hemisphere(n):
    """`n` fixed cosine-weighted directions about +Z (a Fibonacci spiral: deterministic)."""
    golden = math.pi * (3.0 - math.sqrt(5.0))
    out = []
    for i in range(n):
        r = math.sqrt((i + 0.5) / n)
        phi = i * golden
        out.append(Vector((r * math.cos(phi), r * math.sin(phi), math.sqrt(max(0.0, 1.0 - r * r)))))
    return out


def _visibility(tree, point, n, rays):
    """The cosine-weighted share of the hemisphere about `n` seen from `point` unoccluded
    within AO_DISTANCE."""
    t = n.orthogonal().normalized()
    b = n.cross(t)
    origin = point + n * 1e-4
    hits = 0
    for d in rays:
        direction = t * d.x + b * d.y + n * d.z
        if tree.ray_cast(origin, direction, AO_DISTANCE)[0] is not None:
            hits += 1
    return 1.0 - hits / len(rays)


def _bevels(bm):
    """The faces that are bevels: no wider than BEVEL_WIDTH, with convex neighbours across
    both of their long edges (a chamfer between two faces, or a thin plate's edge)."""
    out = set()
    for f in bm.faces:
        edges = sorted(f.edges, key=lambda e: e.calc_length(), reverse=True)
        longest = edges[0].calc_length()
        if longest <= 0 or f.calc_area() / longest > BEVEL_WIDTH:
            continue
        convex = 0
        for e in edges[:2]:
            others = [g for g in e.link_faces if g is not f]
            if len(others) == 1:
                g = others[0]
                if f.normal.dot(g.calc_center_median() - f.calc_center_median()) < 0 and \
                        e.calc_face_angle() > SHARP:
                    convex += 1
        if convex == 2:
            out.add(f.index)
    return out


def _grain_axes(axis, n):
    """The axes u and v of a face with normal `n` on a part whose grain runs along `axis`."""
    facing = max(range(3), key=lambda a: abs(n[a]))
    if axis != facing:
        return axis, next(a for a in range(3) if a not in (axis, facing))
    # End grain: the grain runs into the face.
    rest = [a for a in range(3) if a != facing]
    return rest[0], rest[1]


def bake(obj, rays=AO_RAYS, base=None):
    """Writes the "Color" attribute and the "UVMap", per face corner, before the faces are
    triangulated (common.finish).

    Per corner, not per vertex: on low-poly hard-surface models every vertex lies on an edge or
    under a neighbouring part, and per-vertex values smear across whole faces (the crate: mean
    occlusion 0.25 and wear on 89 % of it; ADR-0026). Occlusion is sampled a fifth of the way from
    the corner to its face's centre, along the corner's own normal; a face larger than
    UNIFORM_AREA takes the mean of its corners, since corners that differ drew dark diagonals
    across its triangles. Wear is full on bevel faces (a band as wide as the bevel). Alpha is the
    corner's height above the model's base over HEIGHT_RANGE: linear, so it interpolates exactly,
    and the renderer turns it into the ground's splash per pixel.

    `rays` a corner (fewer for a character's many corners, whose larger occlusion is the
    renderer's); heights from `base` (m; the object's lowest point by default: a piece of a
    larger model, such as goggles, measures from the model's)."""
    mesh = obj.data
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bm.faces.ensure_lookup_table()
    tree = BVHTree.FromBMesh(bm)
    rays = _hemisphere(rays)
    bevels = _bevels(bm)
    bm.free()
    normals = [c.vector.copy() for c in mesh.corner_normals]
    parts = mesh.attributes["part"].data if "part" in mesh.attributes else None
    grains = mesh.attributes["grain"].data if "grain" in mesh.attributes else None
    bottom = min(v.co.z for v in mesh.vertices) if base is None else base
    colours = [(1.0, 0.0, 0.5, 0.0)] * len(mesh.loops)
    uvs = [(0.0, 0.0)] * len(mesh.loops)
    for poly in mesh.polygons:
        centre = poly.center
        part = parts[poly.index].value if parts else 0
        axis = grains[poly.index].value if grains else 0
        u_axis, v_axis = _grain_axes(axis, poly.normal)
        offset = (hash01(part, 2) * 50.0, hash01(part, 3) * 50.0)
        tone = hash01(part, 1)
        wear = 1.0 if poly.index in bevels else 0.0
        occlusion = {}
        for li in poly.loop_indices:
            corner = mesh.vertices[mesh.loops[li].vertex_index].co
            n = normals[li].normalized()
            # A corner tucked under another part (a batten under a corner plate) is sampled
            # further in, where the face can be seen.
            for share in (0.2, 0.35, 0.5):
                point = corner + (centre - corner) * share
                if tree.ray_cast(point + n * 1e-4, n, COVERED)[0] is None:
                    break
            occlusion[li] = _visibility(tree, point, n, rays)
        if poly.area > UNIFORM_AREA:
            mean = sum(occlusion.values()) / len(occlusion)
            occlusion = dict.fromkeys(occlusion, mean)
        for li in poly.loop_indices:
            corner = mesh.vertices[mesh.loops[li].vertex_index].co
            height = min(1.0, max(0.0, (corner.z - bottom) / HEIGHT_RANGE))
            colours[li] = (occlusion[li], wear, tone, height)
            uvs[li] = (corner[u_axis] + offset[0], corner[v_axis] + offset[1])
    attribute = mesh.color_attributes.get("Color")
    if attribute:
        mesh.color_attributes.remove(attribute)
    attribute = mesh.color_attributes.new("Color", "FLOAT_COLOR", "CORNER")
    for k, c in enumerate(colours):
        attribute.data[k].color = c
    mesh.color_attributes.active_color = attribute
    layer = mesh.uv_layers.get("UVMap") or mesh.uv_layers.new(name="UVMap")
    for k, uv in enumerate(uvs):
        layer.data[k].uv = uv
