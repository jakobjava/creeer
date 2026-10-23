# The light beard tried on the engineer (G1.3f): a function of `art/engineer/hair.py`, called
# from `outfit.body` beside the haircut and brows, before the user: "The beard looks bad, remove
# it". Kept for ADR-0026's "Measured and rejected"; it needs that module's imports and constants.

def beard(scene, skin, eyes, centre):
    """A light beard: jaw, chin, upper lip and sideburns, the cheeks clean above a line from
    the ear to the mouth's corner, the lips bare, short and even."""
    eye = sum((v.co for e in eyes for v in e.data.vertices), Vector()) / sum(len(e.data.vertices) for e in eyes)
    jaw, cheek = BEARD
    lips = (eye.z - 0.076, eye.z - 0.054)

    def depth(v):
        c = v.co
        x, rel = abs(c.x), c.z - eye.z
        if c.y > centre.y + 0.035:
            return 0.0
        # The cheek line: from the sideburn's foot by the ear (7 cm out, 2 cm below the eyes)
        # down to the mouth's corner (3 cm out, 5 cm below), level under the nose.
        line = -0.05 + min(max((x - 0.03) / 0.04, 0.0), 1.0) * 0.03
        below = 1.0 - G.smoothstep(line - 0.008, line + 0.002, rel)
        # The sideburns: just before the ears, up to the hair.
        ear = G.smoothstep(0.064, 0.07, x) * G.smoothstep(eye.y + 0.06, eye.y + 0.075, c.y)
        below = max(below, ear * (1.0 - G.smoothstep(0.012, 0.024, rel)))
        neck = G.smoothstep(-0.13, -0.115, rel)
        behind = 1.0 - G.smoothstep(centre.y + 0.01, centre.y + 0.035, c.y)
        bare = (1.0 - G.smoothstep(0.022, 0.028, x)) * G.smoothstep(lips[0] - 0.002, lips[0] + 0.002, c.z) * (1.0 - G.smoothstep(lips[1] - 0.002, lips[1] + 0.002, c.z))
        full = G.smoothstep(line - 0.03, line - 0.012, rel)
        # Even: thinned in patches, a shell sinks under the skin in blotches.
        return below * neck * behind * (1.0 - bare) * (cheek + (jaw - cheek) * full)

    keep = lambda cloth, v: eye.z - 0.15 < v.co.z < eye.z + 0.04 and v.co.y < centre.y + 0.05 and cloth.arm(v) < 0.01  # noqa: E731
    return grown(scene, skin, keep, depth, "beard", eye + Vector((0.0, 0.0, 0.03)), 0.0004)
