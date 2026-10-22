"""The engineer's surface masks (G1.3f; `docs/art-style.md` §6; the user: "texturise the player
like we did with the crates"): what the renderer's layers need to make worn workwear of the
model, over what the bake gives (occlusion, wear on bevels, each part's tone, the height).

- **Wear** (the colour's green) where workwear wears through, as the renderer reads it: patchy
  chips revealing the token it wears to (canvas to faded canvas), more as it ages. On the knees
  (kneeling), the elbows (leaning), the seat; along every cuff, hem and cut edge (fraying); on
  the boots' toes and heels (scuffs); on the gloves' fingertips and palms. The bake already
  wears the bevels: a patch's, a pocket's and a flap's edges.
- **Tone** (blue): each piece of cloth its own, as cloth cut from different bolts and faded
  differently is (`relief.panel` and `models.build` number the parts; the bake tones them).

In the ragdoll's frame, Blender's axes.
"""

import math

import numpy as np
from mathutils.kdtree import KDTree

import relief as R

SIDES = ((1, "L"), (-1, "R"))
# Zones (m): where each wears most and how far round it.
KNEE = ((0.0, -0.06, 0.0), 0.09)
ELBOW = ((0.0, 0.05, 0.0), 0.075)
SEAT = ((0.075, 0.1, 0.86), 0.09)
FRAY = (0.004, 0.025)
TOE, HEEL = 0.05, 0.035
FINGERS = 0.1


def zone(co, centre, reach):
    """1 at `centre`, falling to 0 `reach` (m) away."""
    d = np.linalg.norm(co - np.array(centre), axis=1)
    return 1.0 - R.smoothstep(0.0, reach, d)


def edges_of(mesh, co):
    """How far (m) each vertex lies from the nearest open edge (a cuff, a hem)."""
    uses = {}
    for p in mesh.polygons:
        for key in p.edge_keys:
            uses[key] = uses.get(key, 0) + 1
    rim = sorted({i for key, n in uses.items() if n == 1 for i in key})
    if not rim:
        return np.full(len(co), np.inf)
    tree = KDTree(len(rim))
    for i in rim:
        tree.insert(co[i], i)
    tree.balance()
    return np.array([tree.find(c)[2] for c in co])


def wear(obj, piece):
    """`obj`'s wear (one level of `piece`, its colour per vertex) raised in the zones."""
    mesh = obj.data
    colour = mesh.color_attributes.get("Color")
    if colour is None or colour.domain != "POINT":
        return
    co, no = R.arrays(mesh)
    out = np.zeros(len(co))
    if piece in ("coverall", "jacket", "base_layer", "boots"):
        out = np.maximum(out, 1.0 - R.smoothstep(*FRAY, edges_of(mesh, co)))
    for side, tag in SIDES:
        if piece == "coverall":
            knee = R.joint(f"knee.{tag}") + np.array(KNEE[0])
            out = np.maximum(out, zone(co, knee, KNEE[1]) * (no[:, 1] < -0.2))
            x, y, z = SEAT[0]
            out = np.maximum(out, 0.8 * zone(co, (side * x, y, z), SEAT[1]) * (no[:, 1] > 0.2))
        if piece == "jacket":
            elbow = R.joint(f"elbow.{tag}") + np.array(ELBOW[0])
            out = np.maximum(out, zone(co, elbow, ELBOW[1]) * (no[:, 1] > 0.2))
        if piece == "boots":
            import last

            ankle, ahead = last.frame(tag)
            u = (co - np.array(ankle)) @ np.array(ahead)
            mine = side * co[:, 0] > 0
            tip, back = u[mine].max(), u[mine].min()
            scuff = np.maximum(R.smoothstep(tip - TOE, tip, u), 1.0 - R.smoothstep(back, back + HEEL, u))
            out = np.maximum(out, mine * scuff * (co[:, 2] < 0.12))
        if piece == "gloves":
            wrist = R.joint(f"wrist.{tag}")
            reach = np.linalg.norm(co - wrist, axis=1)
            palm = no @ np.array((-side, 0.0, 0.0)) > 0.3
            out = np.maximum(out, (side * co[:, 0] > 0) * np.maximum(R.smoothstep(FINGERS, FINGERS + 0.04, reach), 0.6 * palm))
    values = np.empty(len(co) * 4)
    colour.data.foreach_get("color", values)
    values = values.reshape(-1, 4)
    values[:, 1] = np.maximum(values[:, 1], np.clip(out, 0.0, 1.0))
    colour.data.foreach_set("color", values.ravel())


def noise(co, scale, seed):
    """Smooth value noise in [0, 1] at `scale` (cells a metre) over the points `co` (n, 3),
    the same every build (`seed`)."""
    p = co * scale
    i = np.floor(p).astype(np.int64)
    f = p - i
    f = f * f * (3.0 - 2.0 * f)

    def corner(dx, dy, dz):
        x = (i[:, 0] + dx) * 73856093 ^ (i[:, 1] + dy) * 19349663 ^ (i[:, 2] + dz) * 83492791 ^ seed * 2654435761
        x = (x ^ (x >> 13)) * 1274126177
        return ((x ^ (x >> 16)) & 0xFFFF) / 65535.0

    out = np.zeros(len(co))
    for dx in (0, 1):
        for dy in (0, 1):
            for dz in (0, 1):
                w = (f[:, 0] if dx else 1 - f[:, 0]) * (f[:, 1] if dy else 1 - f[:, 1]) * (f[:, 2] if dz else 1 - f[:, 2])
                out += w * corner(dx, dy, dz)
    return out


# The pieces whose cloth and leather stain and fade (not the skin, the hair, the steel).
STAINED = ("coverall", "jacket", "gloves", "boots", "belt", "knee_pads", "elbow_pads", "base_layer")


def stains(obj, piece):
    """Each piece's tone (the colour's blue) varied across it: stains and fading in patches of
    about 14 cm and 4.5 cm, over most of the ±18 % the tone may move (art-style.md §6), round
    each part's own tone. One tone a part left whole garments flat (the user: "a lot less
    monochrome")."""
    if piece not in STAINED:
        return
    colour = obj.data.color_attributes.get("Color")
    if colour is None or colour.domain != "POINT":
        return
    co, _ = R.arrays(obj.data)
    values = np.empty(len(co) * 4)
    colour.data.foreach_get("color", values)
    values = values.reshape(-1, 4)
    patches = 0.6 * (noise(co, 7.0, 11) - 0.5) + 0.4 * (noise(co, 22.0, 23) - 0.5)
    values[:, 2] = np.clip(0.5 + 0.4 * (values[:, 2] - 0.5) + 1.1 * patches, 0.0, 1.0)
    colour.data.foreach_set("color", values.ravel())


def hair_grain(obj):
    """The hair's grain coordinates along the way it lies (the Hair pattern's strands run along
    u): on the head out from the crown (top and back), so they part there in a whorl and fall
    forward, down the sides and the back; on the brows out from the nose.
    Each face's corners unwrapped round its middle, so none straddles the crown's seam."""
    mesh = obj.data
    names = [m.name.split(".")[0] if m else "" for m in mesh.materials]
    faces = [f for f in mesh.polygons if names and names[f.material_index] == "hair"]
    if not faces:
        return
    co = np.array([v.co[:] for v in mesh.vertices])
    used = sorted({i for f in faces for i in f.vertices})
    top = co[used, 2].max()
    middle_y = co[used][co[used, 2] > top - 0.12][:, 1].mean()
    crown = np.array((0.0, middle_y + 0.03, top - 0.01))
    axis = crown - np.array((0.0, middle_y, top - 0.1))
    axis /= np.linalg.norm(axis)
    e1 = np.cross(axis, (1.0, 0.0, 0.0))
    e1 /= np.linalg.norm(e1)
    e2 = np.cross(axis, e1)
    uv = mesh.uv_layers["UVMap"]

    def place(p, around):
        if p[2] > top - 0.115 or p[1] > middle_y - 0.06:
            d = p - crown
            a = math.atan2(d @ e2, d @ e1)
            a = around + (a - around + math.pi) % (2 * math.pi) - math.pi
            return np.linalg.norm(d), 0.08 * a
        return abs(p[0]), p[2]  # the brows

    for f in faces:
        c = np.array(f.center[:]) - crown
        around = math.atan2(c @ e2, c @ e1)
        for li, vi in zip(f.loop_indices, f.vertices):
            uv.data[li].uv = place(co[vi], around)


def number_parts(objs):
    """Each object of a piece its own parts (the "part" face numbers, offset by a hundred an
    object), so the bake tones each piece of cloth apart."""
    for k, obj in enumerate(objs):
        mesh = obj.data
        if not len(mesh.polygons):
            continue
        part = mesh.attributes.get("part") or mesh.attributes.new("part", "INT", "FACE")
        values = np.zeros(len(mesh.polygons), dtype=np.int32)
        part.data.foreach_get("value", values)
        part.data.foreach_set("value", values + 100 * k)
