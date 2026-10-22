"""The engineer's detail (G1.3f; ADR-0026; `docs/art-style.md` §8): what turns the blockout's
smooth pieces into worn workwear, after `outfit.py` has built, posed and lifted them (the
ragdoll's frame). From the detail survey (ADR-0026, Prior work):

- **The jacket** is a two-tone work jacket, as industrial workwear is cut (and Factorio's
  engineer is coloured: its body in the player's colour, orange by default, FFF-218): the
  player's colour on the body and the upper sleeves, the coverall's navy on the shoulders'
  yoke (as the donkey jacket's leather was, the OED's 1929 "leather shoulders and back"), on
  the lower sleeves over the elbows, on the cuffs, the hem and the collar; a zip down the
  front, two plain chest pockets banded at their tops. Not a field jacket: an olive one with a
  buttoned front and buttoned flap pockets read as a military uniform (the user). It blouses
  over the belt in soft pipe folds, stacks above the cuffs and creases at the inner elbows;
  over the upper back it spans from the shoulders (the body's hollows there showed through:
  the user, "back facing collar bones").
- **The coverall** is heavy canvas, as Carhartt's work trousers are (the user's choice over
  quilting, whose diamonds they disliked): double-layer patches over the knees and the seat,
  seams down each leg, a cargo pocket on each thigh, banded cuffs at the wrists, the legs
  stacked above the boots and creased behind the knees.
- `boots.py` and `gear.py`: the boots, gloves, belt, pads and goggles.

The numbers are where the pieces stand (m), measured on them.
"""

import math

import bmesh
import numpy as np
from mathutils import Vector

import garments as G
import outfit as O
import relief as R
import trims as T

SIDES = ((1, "L"), (-1, "R"))
# The jacket: its yoke's lines (front and back, m up), how far it reaches onto the shoulder;
# the zip's half width; the chest pockets (across from the middle, up) and their bands' depth;
# the cuffs' and the hem's bands; how far above the elbow the lower sleeves' navy begins.
YOKE = (1.40, 1.30, 0.205)
ZIP = 0.008
POCKET = ((0.045, 0.14), (1.24, 1.355), 0.016)
CUFF_BAND, HEM_BAND = 0.06, 0.045
LOWER_SLEEVE = 0.05
# The coverall's cargo pockets (up the thigh, m; front and back edges off the leg's middle)
# and the flaps' depth; its cuffs' band.
THIGH_POCKET = ((0.65, 0.83), (-0.065, 0.05), 0.045)
WRIST_BAND = 0.035
# The double layers: over each knee (m above and below it along the leg, rad either side of
# its front), and over the seat (m up; m across from the middle).
KNEE_PATCH = (0.10, 0.15, 0.95)
SEAT_PATCH = ((0.80, 1.06), 0.15)


def arm(side, tag):
    return R.Limb((f"shoulder.{tag}", f"elbow.{tag}", f"wrist.{tag}"), side)


def leg(side, tag):
    return R.Limb((f"hip.{tag}", f"knee.{tag}", f"ankle.{tag}"), side)


def ends(obj, limb, mask):
    """How far along `limb` (m) the garment `obj` reaches where `mask` holds."""
    co, _ = R.arrays(obj.data)
    t, _, _ = limb.coords(co)
    return float(t[mask > 0.5].max())


def jacket(p, belt_z):
    obj = next(o for o in p.objects["jacket"] if o.name.startswith("jacket"))
    R.subdivide(obj)
    mesh = obj.data
    trunk = R.weights(mesh, ("pelvis", "torso"))
    arms = {tag: R.weights(mesh, (f"upper_arm.{tag}", f"forearm.{tag}", f"hand.{tag}")) for _, tag in SIDES}
    body = trunk * (1.0 - np.clip(arms["L"] + arms["R"], 0, 1))
    co, _ = R.arrays(mesh)
    # The shoulders: from the chest's top in front and the shoulder blades behind.
    girdle = np.maximum(R.smoothstep(1.22, 1.3, co[:, 2]) * R.smoothstep(-0.04, 0.02, co[:, 1]), R.smoothstep(1.36, 1.42, co[:, 2]))
    girdle = girdle * (1.0 - R.smoothstep(0.2, 0.26, np.abs(co[:, 0])))
    neck = np.hypot(co[:, 0], co[:, 1] + 0.01)
    R.span(mesh, girdle, most=0.02, frame=girdle * R.smoothstep(0.085, 0.095, neck))
    G.soften(mesh, 60, where=girdle)
    # Round the neck, the cloth taut from the neckline to the shoulders (plain Laplacian: it
    # spans the hollows above the collarbones, which the jacket had followed in).
    co, _ = R.arrays(mesh)
    neck = np.hypot(co[:, 0], co[:, 1] + 0.01)
    band = R.smoothstep(1.4, 1.45, co[:, 2]) * (1.0 - R.smoothstep(0.12, 0.15, neck))
    G.soften(mesh, 200, shrink=0.0, where=band)
    R.blouse(mesh, belt_z, body)
    for side, tag in SIDES:
        limb = arm(side, tag)
        end = ends(obj, limb, arms[tag])
        elbow = limb.lengths[0]
        R.rings(mesh, limb, end - 0.19, end - CUFF_BAND - 0.005, 0.034, 0.004, arms[tag], seed=side)
        R.rings(mesh, limb, elbow - 0.07, elbow + 0.07, 0.026, 0.0035, arms[tag], window=(0.65 * math.pi, 1.0), seed=2 * side)
    navy = R.material_index(obj, O.token("coverall"))
    front = lambda bm: (lambda v: v.co.y < -0.02 and v.co.z > O.HEM)  # noqa: E731
    R.panel(obj, [((ZIP, 0, 0), (1, 0, 0)), ((-ZIP, 0, 0), (-1, 0, 0))], 0.002, front, navy)
    for side, _ in SIDES:
        (x0, x1), (z0, z1), band_depth = POCKET

        def chest(bm, side=side):
            return lambda v: v.co.y < -0.02 and side * v.co.x > ZIP and 1.2 < v.co.z < 1.42

        box = [((side * x0, 0, 0), (-side, 0, 0)), ((side * x1, 0, 0), (side, 0, 0)), ((0, 0, z0), (0, 0, -1)), ((0, 0, z1), (0, 0, 1))]
        R.panel(obj, box, 0.003, chest)
        R.panel(obj, box[:2] + [((0, 0, z1 - band_depth), (0, 0, -1)), box[3]], 0.0, chest, navy)
    yoke_front, yoke_back, reach = YOKE
    slope = Vector((0, -(yoke_front - yoke_back), -0.24)).normalized()
    def shoulders(bm):
        # The planes alone bound it: a weight's edge follows the faces, and frayed its sides.
        return lambda v: v.co.z > 1.2 and not (abs(v.co.x) < ZIP + 1e-4 and v.co.y < -0.02)
    # Its sides lean in a little going down, as an armhole seam does.
    sides = [((reach, 0, 1.45), (0.97, 0, -0.25)), ((-reach, 0, 1.45), (-0.97, 0, -0.25))]
    R.panel(obj, [((0, 0, 0.5 * (yoke_front + yoke_back)), slope)] + sides, 0.0, shoulders, navy)
    for side, tag in SIDES:
        limb = arm(side, tag)
        centre, axis = limb.at(limb.lengths[0] - LOWER_SLEEVE)

        def lower(bm, tag=tag):
            w = R.layer(bm, (f"upper_arm.{tag}", f"forearm.{tag}", f"hand.{tag}"))
            return lambda v: w(v) > 0.5

        R.panel(obj, [(centre, -axis)], 0.0, lower, navy)
        end = ends(obj, limb, R.weights(mesh, (f"forearm.{tag}", f"hand.{tag}")))
        centre, axis = limb.at(end - CUFF_BAND)
        def cuff(bm, tag=tag):
            w = R.layer(bm, (f"forearm.{tag}", f"hand.{tag}"))
            return lambda v: w(v) > 0.5
        R.panel(obj, [(centre, -axis)], 0.0025, cuff)
    hem = min(v.co.z for v in mesh.vertices)
    def band(bm):
        # Not by the trunk's weight: at the hips the thighs share it.
        w = R.layer(bm, tuple(f"{s}.{t}" for s in ("upper_arm", "forearm", "hand") for _, t in SIDES))
        return lambda v: w(v) < 0.5 and v.co.z < hem + 0.08
    R.panel(obj, [((0, 0, hem + HEM_BAND), (0, 0, 1))], 0.0025, band, navy)
    zip_pull(p, obj)


def zip_pull(p, host):
    """The zip's steel slider at its top, under the collar, its pull hanging from it."""
    surface = T.Surface(host)
    top = max(v.co.z for v in host.data.vertices if abs(v.co.x) < ZIP and v.co.y < -0.02)
    hit, normal = surface.cast((0, -0.3, top - 0.03), (0, 1, 0))
    if hit is None:
        return
    bm = bmesh.new()
    T.dome(bm, hit + normal * 0.002, normal, 0.007, 0.004)
    T.ring(bm, hit + normal * 0.006 - Vector((0, 0, 0.012)), Vector((1, 0, 0)), 0.006, 0.0015, segments=10, sides=4)
    T.place(p, "jacket", "zip", bm, "steel", host)["hardware"] = "close"


def coverall(p):
    obj = next(o for o in p.objects["coverall"] if o.name.startswith("coverall"))
    R.subdivide(obj)
    mesh = obj.data
    co, _ = R.arrays(mesh)
    # The seat spanned over the buttocks (their cleft and the fold beneath), the crotch hung
    # across between the legs (taut: plain Laplacian), not following the body into either (the
    # user); then kept off the body.
    seat = R.smoothstep(0.02, 0.05, co[:, 1]) * R.smoothstep(0.72, 0.76, co[:, 2]) * (1.0 - R.smoothstep(0.95, 0.99, co[:, 2]))
    seat = seat * (1.0 - R.smoothstep(0.13, 0.17, np.abs(co[:, 0])))
    R.span(mesh, seat, most=0.035)
    G.soften(mesh, 40, where=seat)
    co, _ = R.arrays(mesh)
    crotch = (1.0 - R.smoothstep(0.06, 0.11, np.abs(co[:, 0]))) * R.smoothstep(0.72, 0.78, co[:, 2]) * (1.0 - R.smoothstep(0.93, 0.98, co[:, 2]))
    G.soften(mesh, 300, shrink=0.0, where=crotch)
    R.over(obj, next(o for o in p.objects["body"] if o.name == "skin"), 0.006)
    co, _ = R.arrays(mesh)
    for side, tag in SIDES:
        limb = leg(side, tag)
        legs = R.weights(mesh, (f"thigh.{tag}", f"shank.{tag}"))
        (z0, z1), (y0, y1), _ = THIGH_POCKET
        pocket = (co[:, 2] > z0 - 0.01) & (co[:, 2] < z1 + 0.01) & (co[:, 1] > y0 - 0.01) & (co[:, 1] < y1 + 0.01) & (side * co[:, 0] > 0.1)
        knee = limb.lengths[0]
        R.seams(mesh, limb, legs * ~pocket)
        # Stacked above the boot's top (inside it the leg is held, and folds there came out
        # through its collar).
        boot = next(o for o in p.objects["boots"] if o.name.startswith("boots"))
        top = max(v.co.z for v in boot.data.vertices)
        ankle = R.joint(f"ankle.{tag}")
        start = float(limb.coords(np.array([(ankle[0], ankle[1], top + 0.012)]))[0][0])
        R.rings(mesh, limb, start - 0.1, start, 0.04, 0.0025, legs, seed=side, wobble=1.8)
        R.rings(mesh, limb, knee - 0.06, knee + 0.06, 0.024, 0.003, legs, window=(-0.5 * math.pi, 1.1), seed=3 * side)
        co, _ = R.arrays(mesh)
    patches(obj)
    for side, tag in SIDES:
        (z0, z1), (y0, y1), flap = THIGH_POCKET
        def thigh(bm, side=side, tag=tag):
            w = R.layer(bm, (f"thigh.{tag}",))
            return lambda v: w(v) > 0.5 and side * v.co.x > 0.1
        box = [((0, 0, z0), (0, 0, -1)), ((0, 0, z1), (0, 0, 1)), ((0, y0, 0), (0, -1, 0)), ((0, y1, 0), (0, 1, 0))]
        R.panel(obj, box, 0.003, thigh)
        R.panel(obj, [((0, 0, z1 - flap), (0, 0, -1))] + box[1:], 0.0025, thigh)
    for side, tag in SIDES:
        limb = arm(side, tag)
        sleeves = R.weights(mesh, (f"forearm.{tag}", f"hand.{tag}"))
        end = ends(obj, limb, sleeves)
        R.rings(mesh, limb, end - 0.11, end - WRIST_BAND - 0.004, 0.028, 0.003, sleeves, seed=4 * side)
        centre, axis = limb.at(end - WRIST_BAND)
        def wrist(bm, tag=tag):
            w = R.layer(bm, (f"forearm.{tag}", f"hand.{tag}"))
            return lambda v: w(v) > 0.5
        R.panel(obj, [(centre, -axis)], 0.002, wrist)


def patches(obj):
    """The coverall's double layers over the knees and the seat."""
    above, below, half = KNEE_PATCH
    for side, tag in SIDES:
        limb = leg(side, tag)
        knee = Vector(R.joint(f"knee.{tag}"))
        axis = Vector(limb.points[2] - limb.points[0]).normalized()
        out, front = (Vector(x) for x in limb.frames[0])
        planes = [(knee - axis * above, -axis), (knee + axis * below, axis)]
        for a in (0.5 * math.pi - half, 0.5 * math.pi + half):
            radial = out * math.cos(a) + front * math.sin(a)
            n = axis.cross(radial).normalized()
            planes.append((knee, n if n.dot(front) < 0 else -n))

        def on_leg(bm, tag=tag):
            w = R.layer(bm, (f"thigh.{tag}", f"shank.{tag}"))
            return lambda v: w(v) > 0.5

        R.panel(obj, planes, 0.002, on_leg)
    (z0, z1), half = SEAT_PATCH

    def seat(bm):
        w = R.layer(bm, ("pelvis", "thigh.L", "thigh.R"))
        return lambda v: w(v) > 0.5 and v.co.y > 0.0

    R.panel(obj, [((0, 0, z0), (0, 0, -1)), ((0, 0, z1), (0, 0, 1)), ((half, 0, 0), (1, 0, 0)), ((-half, 0, 0), (-1, 0, 0)), ((0, 0.03, 0), (0, -1, 0))], 0.002, seat)


def cinch(belt, cloths, clear=0.003, fall=0.015, bins=72):
    """The trunk's cloth under the belt drawn in within it, `clear` (m) inside its inner face,
    each of `cloths` (outermost first) 4 mm further in, easing out over `fall` (m) above and
    below: a belt gathers what it is buckled over. Ringed round the jacket before its folds and
    the coverall's ease were added, the belt had them through it by 7 mm."""
    bo, _ = R.arrays(belt.data)
    z0, z1 = bo[:, 2].min(), bo[:, 2].max()
    middle = bo[:, :2].mean(axis=0)

    def angle(co):
        return (np.floor((np.arctan2(co[:, 1] - middle[1], co[:, 0] - middle[0]) + math.pi) / (2 * math.pi) * bins).astype(int)) % bins

    inner = np.full(bins, np.inf)
    np.minimum.at(inner, angle(bo), np.linalg.norm(bo[:, :2] - middle, axis=1))
    found = np.nonzero(np.isfinite(inner))[0]
    inner = np.interp(np.arange(bins), found, inner[found], period=bins)
    for k, obj in enumerate(cloths):
        co, _ = R.arrays(obj.data)
        arms = R.weights(obj.data, tuple(f"{s}.{t}" for s in ("upper_arm", "forearm", "hand") for _, t in SIDES))
        held = (1.0 - np.clip(arms, 0.0, 1.0)) * R.smoothstep(z0 - fall, z0, co[:, 2]) * (1.0 - R.smoothstep(z1, z1 + fall, co[:, 2]))
        d = co[:, :2] - middle
        r = np.linalg.norm(d, axis=1)
        target = inner[angle(co)] - clear - 0.004 * k
        new = r - np.maximum(r - target, 0.0) * held
        co[:, :2] = middle + d * (new / np.maximum(r, 1e-9))[:, None]
        obj.data.vertices.foreach_set("co", co.ravel())
        obj.data.update()


def detail(scene, pieces):
    """Every piece's detail, in place; trims added to their pieces. The boots first: the
    coverall's legs stack above their tops."""
    import boots
    import gear
    import last

    p = O.Pieces(scene)
    p.objects = pieces
    boots.detail(p)
    belt = pieces["belt"][0]
    belt_z = sum(v.co.z for v in belt.data.vertices) / len(belt.data.vertices)
    jacket(p, belt_z)
    coverall(p)
    cinch(belt, [next(o for o in pieces[w] if o.name.startswith(w)) for w in ("jacket", "coverall")])
    # Last, once the coverall is subdivided and detailed: its legs bloused into the boots.
    last.tuck(pieces["coverall"][0], next(o for o in pieces["boots"] if o.name == "boots"), last.SHAFT[1])
    gear.detail(p, belt_z)
    return pieces
