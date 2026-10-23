"""The engineer's hair (G1.3f; the user: "proper hair and facial hair (eye brows and maybe a light
beard)"): grown from the skin as the garments are, each a shell in the `hair` token, its edge
sunk under the skin so it fades in rather than ending in a step; clumped along the way it is
combed, its strands in its surface (the Hair pattern, along the grain `surface.hair_grain`
lays). A light beard was made and taken out (the user: "the beard looks bad, remove it";
ADR-0026, Measured and rejected).

- **The haircut**: short at the back and sides, longer on top (about 7 mm to 17 mm), combed
  from the crown forward and down; its hairline tufted.
- **The eyebrows**: arched over each eye, thickest at the inner end, their hairs slanting up and
  out.

The brows are placed from the eyes (the body fitted to ANSUR II): 2.2 cm above them.
"""

import math

import numpy as np
from mathutils import Vector

import garments as G
import surface as Sf

# The haircut's depth (m) at the back and sides and on top, its locks' depth, and how far (m)
# it fades in from the hairline (short back and sides: a taper, not a brim). The strands are
# the surface's (the Hair pattern, along the grain `surface.hair_grain` lays).
CROP, TOP, CLUMPS, FADE = 0.006, 0.018, 0.0025, 0.03
# Brows: from 1.2 cm to 6.2 cm off the middle, 2.2 cm above the eyes at their peak, half as
# tall as they are thick inside (m).
BROW = ((0.012, 0.062), 0.022, 0.005, 0.0025)


def combed(co, normals, crown, along=4.0, scale=32.0, seed=5):
    """Locks (0–1) stretched along the way the hair is combed (away from `crown`, along the
    surface): noise squeezed `along` times along it, `scale` cells a metre across (about 3 cm
    locks). Ridged, they stood up as spikes."""
    d = co - np.array(crown)
    d = d - normals * (d * normals).sum(axis=1)[:, None]
    t = d / np.maximum(np.linalg.norm(d, axis=1), 1e-9)[:, None]
    q = co - t * (co * t).sum(axis=1)[:, None] * (1.0 - 1.0 / along)
    return Sf.noise(q, scale, seed)


def grown(scene, skin, keep, depth, name, crown, clumps):
    """A shell grown from the skin where `keep(cloth, v)` holds, `depth(v)` (m) out, sunk 3 mm
    under the skin where its depth fades to nothing, clumped along the combing from `crown`."""
    cloth = G.Cloth(skin)
    cloth.keep(lambda v: keep(cloth, v))
    cloth.bm.normal_update()
    co = np.array([v.co[:] for v in cloth.bm.verts])
    no = np.array([v.normal[:] for v in cloth.bm.verts])
    texture = combed(co, no, crown) - 0.5
    lifts = {}
    for k, v in enumerate(cloth.bm.verts):
        d = depth(v)
        lifts[v] = d + clumps * texture[k] * min(1.0, d / 0.004) - 0.003 * (1.0 - min(1.0, d / 0.001))
    cloth.push(lambda v: lifts[v])
    sunk = {v for v, lift in lifts.items() if lift < -0.0025}
    cloth.drop(lambda v: v in sunk)
    return cloth.make(scene, name, 0.0, edge=0.0)


def haircut(scene, skin, centre):
    """Short hair: deeper on top, the hairline tufted."""
    crown = centre + Vector((0.0, 0.035, 0.07))
    tuft = lambda v: 0.016 * (Sf.noise(np.array([v.co[:]]), 55.0, 7)[0] - 0.5)  # noqa: E731

    def depth(v):
        d = v.co - centre
        front = max(0.0, -d.y / max(d.length, 1e-9))
        line = G.hairline(d) + tuft(v) * (0.6 + 0.8 * front)
        # The fringe ends in tufts; the back and sides fade in.
        fade = 0.008 + (FADE - 0.008) * (1.0 - front)
        rise = G.smoothstep(line - 0.008, line + fade, d.z)
        top = G.smoothstep(0.0, 0.07, d.z)
        return rise * (CROP + (TOP - CROP) * top)

    keep = lambda cloth, v: v.co.z > centre.z - 0.09 and cloth.arm(v) < 0.01  # noqa: E731
    return grown(scene, skin, keep, depth, "hair", crown, CLUMPS)


def brows(scene, skin, eyes):
    """The eyebrows, arched over the eyes."""
    eye = sum((v.co for e in eyes for v in e.data.vertices), Vector()) / sum(len(e.data.vertices) for e in eyes)
    (inner, outer), above, tall, thin = BROW

    def depth(v):
        x = abs(v.co.x)
        if v.co.y > eye.y + 0.01 or not inner - 0.004 < x < outer + 0.004:
            return 0.0
        f = min(max((x - inner) / (outer - inner), 0.0), 1.0)
        middle = eye.z + above - 0.004 + 0.006 * math.sin(math.pi * min(1.0, f * 1.3))
        half = tall + (thin - tall) * f
        across = 1.0 - G.smoothstep(half * 0.6, half, abs(v.co.z - middle))
        ends = G.smoothstep(inner - 0.004, inner + 0.004, x) * (1.0 - G.smoothstep(outer - 0.008, outer + 0.004, x))
        return 0.0026 * (1.0 - 0.45 * f) * across * ends

    keep = lambda cloth, v: abs(v.co.z - eye.z - above) < 0.02 and v.co.y < eye.y + 0.02  # noqa: E731
    return grown(scene, skin, keep, depth, "brows", eye + Vector((0.0, 0.0, -0.05)), 0.0008)
