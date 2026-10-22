"""What the hand tools are made of (G1.4c; ADR-0027 decision 3; docs/art-style.md §8, "Tools"):
wooden handles and forged steel heads, simple and sturdy, each tool's shape unmistakable.

Every tool stands with its handle along +Z (the game's +Y), its origin where the right hand
grips it, its working end (the pick's point, the axe's edge, the shovel's blade, the saw's
teeth) facing −Y (the game's forward, +Z). Their sizes and masses are catalogue tools'
(`data/models/models.ron`, `data/tools.ron`).

The shapes are lofts: rings of points round a centre, joined ring to ring, each ring 24 points
round (art-style.md §7: a full circle at least 24 segments), the ends closed by a smaller ring
and a cap, so a handle's end is rounded, not cut.
"""

import math

import bmesh
from common import _last_part
from mathutils import Vector

ROUND = 24


def ring(centre, u, v, a, b, n=ROUND, square=0.0):
    """`n` points round `centre` in the plane of `u` and `v`: an ellipse of half widths `a` and
    `b`, squared towards a rectangle by `square` (0 an ellipse, towards 1 a rounded rectangle)."""
    out = []
    for k in range(n):
        t = 2.0 * math.pi * k / n
        c, s = math.cos(t), math.sin(t)
        if square:
            e = 2.0 / (2.0 + 6.0 * square)
            c = math.copysign(abs(c) ** e, c)
            s = math.copysign(abs(s) ** e, s)
        out.append(centre + u * (a * c) + v * (b * s))
    return out


def loft(bm, rings, grain, caps=(True, True)):
    """Joins `rings` (lists of points, all as long) into one part of `bm`, its grain along axis
    `grain` (0–2), its ends closed by caps where `caps` asks; returns its faces."""
    part = bm.faces.layers.int.get("part") or bm.faces.layers.int.new("part")
    axis = bm.faces.layers.int.get("grain") or bm.faces.layers.int.new("grain")
    verts = [[bm.verts.new(p) for p in r] for r in rings]
    n = len(rings[0])
    faces = []
    for a, b in zip(verts, verts[1:]):
        for k in range(n):
            faces.append(bm.faces.new((a[k], a[(k + 1) % n], b[(k + 1) % n], b[k])))
    if caps[0]:
        faces.append(bm.faces.new(list(reversed(verts[0]))))
    if caps[-1]:
        faces.append(bm.faces.new(verts[-1]))
    _last_part[0] += 1
    for f in faces:
        f[part] = _last_part[0]
        f[axis] = grain
    bm.normal_update()
    return faces


def rounded_ends(sections, end, width=0.004):
    """`sections` (t, a, b) with each end rounded: a section `end` in (in t) kept full and the
    end itself `width` (m) smaller all round, as a turned handle's or a forged bar's ends are."""
    (t0, a0, b0), (t1, a1, b1) = sections[0], sections[-1]
    first = (t0 + end, a0, b0)
    last = (t1 - end, a1, b1)
    return [(t0, a0 - width, b0 - width), first, *sections[1:-1], last, (t1, a1 - width, b1 - width)]


def bar(bm, path, sections, grain, square=0.0, end=0.004):
    """A bar along `path` (a function of t in [0, 1] to a centre point and its frame u, v) of
    `sections` (t, a, b), its ends rounded over `end`."""
    rings = []
    # The rounding along the path as a share of it (`sections` run in t).
    length = (path(1.0)[0] - path(0.0)[0]).length
    for t, a, b in rounded_ends(sections, end / max(length, 1e-6)):
        c, u, v = path(t)
        rings.append(ring(c, u, v, max(a, 0.0005), max(b, 0.0005), square=square))
    return loft(bm, rings, grain)


def straight(start, end, u=Vector((1.0, 0.0, 0.0))):
    """A straight path from `start` to `end`, its frame's u turned off the path's direction."""
    start, end = Vector(start), Vector(end)
    d = (end - start).normalized()
    u = (u - d * u.dot(d)).normalized()
    v = d.cross(u)

    def at(t):
        return start.lerp(end, t), u, v

    return at


def arc(centre, radius, a0, a1, axis=Vector((1.0, 0.0, 0.0))):
    """A path round `axis` through `centre`, at `radius`, from angle `a0` to `a1` (rad), in the
    plane of y and z (0 along −y, turning towards +z)."""

    def at(t):
        a = a0 + (a1 - a0) * t
        r = Vector((0.0, -math.cos(a), math.sin(a)))
        c = Vector(centre) + r * radius
        tangent = Vector((0.0, math.sin(a), math.cos(a))) * (1.0 if a1 > a0 else -1.0)
        u = axis.normalized()
        v = tangent.cross(u)
        return c, u, v

    return at


def volume(obj):
    """The closed mesh's volume (m³)."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    out = bm.calc_volume(signed=False)
    bm.free()
    return out


# The materials' densities (kg/m³): forged steel; hickory air-dry, the handles' wood (0.72–0.83
# in the Wood Handbook's range for the true hickories).
DENSITY = {"steel": 7850.0, "steel_painted": 7850.0, "timber": 800.0}


def report(name, parts):
    """Prints each part's mass (kg) by its material, from its closed mesh's volume: what its
    entry in `data/tools.ron` gives, to compare."""
    total, moment = 0.0, Vector((0.0, 0.0, 0.0))
    wood = [o for o in parts if o.data.materials[0].name == "timber"]
    for obj in parts:
        token = obj.data.materials[0].name
        inside = 0.0
        if token != "timber":
            # The handle through the eye: steel's place is the wood's there.
            inside = sum(overlap(obj, w) for w in wood)
        mass = (volume(obj) - inside) * DENSITY[token]
        if "wall" in obj:
            # A tube (the saw's bow): its outer surface times its wall, not its whole section.
            mass = area(obj) * obj["wall"] * DENSITY[token]
        total += mass
        moment += mass * centroid(obj)
        print(f"art: {name}: {obj.name} ({token}) {mass:.3f} kg")
    print(f"art: {name}: {total:.3f} kg in all")
    # Its balance: where it hangs level from, up the handle from the grip (m).
    print(f"art: {name}: balanced {moment.z / total:.3f} m up from the grip")
    # Held to the catalogue's mass in data/tools.ron (the game's), within 15 %.
    import pathlib
    import re

    table = (pathlib.Path(__file__).resolve().parent.parent / "data" / "tools.ron").read_text(encoding="utf-8")
    found = re.search(rf'model: "{name}",[^)]*?mass: ([0-9.]+)', table)
    if found is None:
        raise ValueError(f"{name}: no entry in data/tools.ron")
    want = float(found.group(1))
    if abs(total - want) > 0.15 * want:
        raise ValueError(f"{name}: {total:.3f} kg, the catalogue's {want} kg: more than 15 % apart")
    # Carried at its balance (from the handle's end, as the table gives it), to within 1 cm.
    entry = re.search(rf'model: "{name}",[^)]*?grip: ([0-9.]+),[^)]*?carry: ([0-9.]+)', table)
    balance = moment.z / total + float(entry.group(1))
    if abs(balance - float(entry.group(2))) > 0.01:
        raise ValueError(f"{name}: balanced {balance:.3f} m up, carried at {entry.group(2)} m")


def overlap(a, b):
    """The volume (m³) the closed meshes `a` and `b` share, by a Boolean intersection of
    copies."""
    import bpy

    copy = a.copy()
    copy.data = a.data.copy()
    bpy.context.scene.collection.objects.link(copy)
    modifier = copy.modifiers.new("inside", "BOOLEAN")
    modifier.operation = "INTERSECT"
    modifier.solver = "EXACT"
    modifier.object = b
    depsgraph = bpy.context.evaluated_depsgraph_get()
    mesh = bpy.data.meshes.new_from_object(copy.evaluated_get(depsgraph), depsgraph=depsgraph)
    bm = bmesh.new()
    bm.from_mesh(mesh)
    out = bm.calc_volume(signed=False)
    bm.free()
    bpy.data.objects.remove(copy)
    bpy.data.meshes.remove(mesh)
    return out


def area(obj):
    """The mesh's surface area (m²)."""
    return sum(p.area for p in obj.data.polygons)


def centroid(obj):
    """The centre of the closed mesh's volume (m), by the signed tetrahedra its faces make with
    the origin; for a tube (`wall`), the centre of its surface."""
    mesh, total, at = obj.data, 0.0, Vector((0.0, 0.0, 0.0))
    for poly in mesh.polygons:
        v = [obj.matrix_world @ mesh.vertices[i].co for i in poly.vertices]
        for k in range(1, len(v) - 1):
            a, b, c = v[0], v[k], v[k + 1]
            if "wall" in obj:
                w = (b - a).cross(c - a).length / 2.0
                at += w * (a + b + c) / 3.0
            else:
                w = a.dot(b.cross(c)) / 6.0
                at += w * (a + b + c) / 4.0
            total += w
    return at / total
