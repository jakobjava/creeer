# The diamond quilting tried on the coverall (G1.3f): a function of `art/engineer/detail.py`,
# called on each leg and sleeve before the user chose canvas ("I hate the diamond shapes").
# Kept for ADR-0026's "Measured and rejected"; it needs that module's imports (math, numpy as np,
# relief as R).

def quilt(mesh, limb, mask, size=0.08, depth=0.004, seams=(0.0, math.pi), seam_depth=0.002):
    """Diamond quilting round the limb: stitched lines crossing at right angles, `size` (m) apart
    along and round it, each pocket of padding puffed between them (`depth`, m, from the lines'
    bottoms to the pockets' tops); the quilting stops at the seams down the limb (rad round it,
    the outseam and the inseam), each a groove `seam_depth` deep. Insulated workwear quilts at
    about 7.5 cm (US patent 4,460,645), the Dickies diamond-quilted jacket alike."""
    co, _ = arrays(mesh)
    t, theta, off = limb.coords(co)
    u = theta * girth(t, off, mask)
    a, b = (u + t) / size, (u - t) / size
    reach = size / math.sqrt(2.0)
    line = np.minimum(np.abs(a - np.round(a)), np.abs(b - np.round(b))) * reach
    # A rounded groove (flat for 3 mm each side of the stitches): a sharp one, sampled by
    # the 5 mm mesh, zigzagged along the diagonals.
    pillow = smoothstep(0.003, 0.3 * reach, line)
    seam = np.min([angle_from(theta, s) for s in seams], axis=0) * girth(t, off, mask)
    fade = smoothstep(0.005, 0.011, seam)
    groove = 1.0 - smoothstep(0.001, 0.005, seam)
    displace(mesh, mask * (depth * (pillow - 0.75) * fade - seam_depth * groove))


