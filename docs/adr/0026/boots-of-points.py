# The boots of points tried (G1.3f): the edit script that rewrote `art/engineer/last.py` to grow
# the upper as a ball round each of the skin's points (Blender's points to SDF grid). Every toe
# showed as a lump. It applied to that file as it then was, not to the final one; kept for
# ADR-0026's "Measured and rejected".

import sys

root = sys.argv[1]
p = f"{root}/art/engineer/last.py"
s = open(p, encoding="utf-8").read()


def cut(a, b):
    """Replaces the text from `a` up to (not including) `b`."""
    global s
    i, j = s.index(a), s.index(b)
    return i, j


i, j = s.index('"""The boots\' shape'), s.index("import math")
s = s[:i] + '''"""The boots' shape (G1.3f; `boots.py`): each boot grown round its own foot, as a last is
made to a foot, then given a work boot's room. The blockout's boot was the foot's convex hull:
low at the toes, high at the ankle, a wedge on a flat sole (the user: "triangles on flip
flops"). Ellipsoids merged into a boot were tried next: a clown's shoe, its toe a ball apart
from its instep, and the big toe through its side (ADR-0026, Measured and rejected).

- **The upper**: the surface 11 mm round the foot and ankle (Blender's points to SDF grid, a
  ball round each of the skin's points, so the foot is inside by its making); a toe box with
  room over the toes (their points again, raised and moved ahead); a straight shaft wide enough
  for the coverall's leg tucked into it (an elliptic ring of points at each height). Smoothed
  (Taubin), cut level with the sole's top and at the shaft's top.
- **The sole**: round the upper's foot 4 mm out (the welt's ledge): rubber lugs round its edge,
  its tread lifted under the arch and at the toe, a leather welt on top.

In a foot's frame: metres ahead of the ankle, out to its side, up from the ground.
"""

''' + s[j:]

i, j = s.index("# The upper (m):"), s.index("SOLE_TOP = 0.03")
s = s[:i] + '''# The upper (m): its leather and padding round the foot; the toe box's room (up and ahead,
# growing from `ROOMY` ahead of the ankle to `ROOMY_FULL`); the shaft from its foot to its top,
# round the coverall's leg there with this much to spare.
LEATHER = 0.011
TOE_ROOM = (0.012, 0.014)
ROOMY = (0.05, 0.16)
SHAFT = (0.09, 0.245, 0.004)
VOXEL = 0.002
''' + s[j:]

i, j = s.index("def upper(scene, tag, side):"), s.index("def outline_of(obj, spacing=0.006):")
s = s[:i] + '''def points_surface(scene, points, radius, voxel):
    """The surface `radius` (m) round `points` (each a ball, merged), meshed at `voxel` (m)."""
    mesh = bpy.data.meshes.new("points")
    mesh.from_pydata([tuple(p) for p in points], [], [])
    obj = bpy.data.objects.new("points", mesh)
    scene.collection.objects.link(obj)
    tree = bpy.data.node_groups.new("points to surface", "GeometryNodeTree")
    tree.interface.new_socket("Geometry", in_out="INPUT", socket_type="NodeSocketGeometry")
    tree.interface.new_socket("Geometry", in_out="OUTPUT", socket_type="NodeSocketGeometry")
    nodes, links = tree.nodes, tree.links
    given, made = nodes.new("NodeGroupInput"), nodes.new("NodeGroupOutput")
    balls = nodes.new("GeometryNodeMeshToPoints")
    balls.inputs["Radius"].default_value = radius
    grid = nodes.new("GeometryNodePointsToSDFGrid")
    grid.inputs["Radius"].default_value = radius
    grid.inputs["Voxel Size"].default_value = voxel
    surface = nodes.new("GeometryNodeGridToMesh")
    links.new(given.outputs[0], balls.inputs["Mesh"])
    links.new(balls.outputs[0], grid.inputs["Points"])
    links.new(grid.outputs[0], surface.inputs["Grid"])
    links.new(surface.outputs[0], made.inputs[0])
    mod = obj.modifiers.new("surface", "NODES")
    mod.node_group = tree
    with bpy.context.temp_override(object=obj, active_object=obj):
        bpy.ops.object.modifier_apply(modifier="surface")
    bpy.data.node_groups.remove(tree)
    return obj


def shaft_ring(leg, centre, z):
    """The coverall's leg round `centre` at height `z`: its reach ahead and to the side (m)."""
    near = leg[np.abs(leg[:, 2] - z) < 0.006]
    if len(near) == 0:
        return None
    return np.abs(near[:, :2] - centre[:2]).max(axis=0)


def upper(scene, skin, coverall, tag, side):
    """The boot's upper: one smooth surface round the foot, open at the shaft's top and at the
    sole."""
    turn, origin = placing(tag, side)
    ahead, out = Vector(turn.col[0][:3]), Vector(turn.col[1][:3])
    foot = feet(skin, tag)
    rel = foot - np.array(origin)
    u = rel @ np.array(ahead)
    room = R.smoothstep(ROOMY[0], ROOMY[1], u)
    toes = foot[room > 0] + np.outer(room[room > 0], np.array(ahead) * TOE_ROOM[1] + np.array((0, 0, TOE_ROOM[0])))
    z0, z1, spare = SHAFT
    co, _ = R.arrays(coverall.data)
    leg = co[(R.weights(coverall.data, (f"thigh.{tag}", f"shank.{tag}")) > 0.5) & (co[:, 2] < z1 + 0.02)]
    centre = np.array(R.joint(f"ankle.{tag}"))
    top = [r for r in (shaft_ring(leg, centre, z) for z in np.linspace(z1 - 0.04, z1, 5)) if r is not None]
    ru, rv = (np.max(top, axis=0) if top else np.array((0.065, 0.06))) + spare - LEATHER
    ring = []
    for z in np.arange(z0, z1 + 0.02, 0.004):
        for a in np.linspace(0, 2 * np.pi, 72, endpoint=False):
            ring.append((centre[0] + ru * np.cos(a), centre[1] + rv * np.sin(a), z))
    obj = points_surface(scene, np.concatenate((foot, toes, np.array(ring))), LEATHER, VOXEL)
    mesh = obj.data
    G.soften(mesh, 30)
    bm = bmesh.new()
    bm.from_mesh(mesh)
    for co_, no in (((0, 0, SOLE_TOP), (0, 0, -1)), ((0, 0, z1), (0, 0, 1))):
        bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], plane_co=co_, plane_no=no, dist=1e-5)
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if (v.co - Vector(co_)).dot(Vector(no)) > 1e-5], context="VERTS")
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(mesh)
    bm.free()
    obj.name = mesh.name = f"boot.{tag}"
    del out
    return obj


def tuck(coverall, boots, top):
    """The coverall's legs below the boots' tops pulled inside them (3 mm in)."""
    group = coverall.vertex_groups.new(name="tuck")
    group.add([v.index for v in coverall.data.vertices if v.co.z < top - 0.004], 1.0, "REPLACE")
    wrap = coverall.modifiers.new("tuck", "SHRINKWRAP")
    wrap.target, wrap.wrap_method, wrap.wrap_mode, wrap.offset = boots, "NEAREST_SURFACEPOINT", "INSIDE", -0.003
    wrap.vertex_group = "tuck"
    with bpy.context.temp_override(object=coverall, active_object=coverall):
        bpy.ops.object.modifier_apply(modifier="tuck")
    coverall.vertex_groups.remove(coverall.vertex_groups["tuck"])


''' + s[j:]
s = s.replace("import bmesh\nimport bpy\nfrom mathutils", "import bmesh\nimport bpy\nimport numpy as np\nfrom mathutils")
i, j = s.index("def feet(skin, tag):"), len(s)
s = s[:i] + '''def feet(skin, tag):
    """The skin's foot and ankle up to the shaft's top, the points the boot must hold."""
    co, _ = R.arrays(skin.data)
    w = R.weights(skin.data, (f"foot.{tag}", f"shank.{tag}"))
    return co[(w > 0.5) & (co[:, 2] < SHAFT[1] + 0.01)]
'''
open(p, "w", encoding="utf-8", newline="\n").write(s)

p = f"{root}/art/engineer/boots.py"
s = open(p, encoding="utf-8").read()
a = '''    uppers = [L.upper(p.scene, tag, side) for side, tag in SIDES]'''
b = '''    coverall = next(o for o in p.objects["coverall"] if o.name.startswith("coverall"))
    uppers = [L.upper(p.scene, skin, coverall, tag, side) for side, tag in SIDES]'''
assert s.count(a) == 1
s = s.replace(a, b)
a = '''    p.add("boots", boots, "leather")'''
b = '''    p.add("boots", boots, "leather")
    L.tuck(coverall, boots, L.SHAFT[1])'''
assert s.count(a) == 1
s = s.replace(a, b)
open(p, "w", encoding="utf-8", newline="\n").write(s)
