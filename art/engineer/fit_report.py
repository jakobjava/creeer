"""What the engineer's fit check prints (`check_fit.py`): its misses grouped, and each piece's
share of points outside its segments' boxes."""

import json
import pathlib

from mathutils import Vector

BONES = json.loads((pathlib.Path(__file__).resolve().parent / "bones.json").read_text(encoding="utf-8"))


def g2b(p):
    return Vector((p[0], -p[2], p[1]))


def boxes(objects):
    """Each piece's share of points outside its segments' boxes (grown by 3 cm for the body, 5
    cm for the rest), by segment, at rest."""
    box = {}
    for seg in BONES["segments"]:
        c, h = g2b(seg["centre"]), seg["half"]
        box[seg["name"]] = (c, Vector((h[0], h[2], h[1])))
    out = {}
    for piece, objs in objects.items():
        grow = 0.03 if piece == "body" else 0.05
        counts = {}
        for obj in objs:
            names = {g.index: g.name for g in obj.vertex_groups}
            for v in obj.data.vertices:
                if not v.groups:
                    continue
                seg = names[max(v.groups, key=lambda g: g.weight).group]
                outside = True
                for g in v.groups:
                    if g.weight >= 0.2:
                        c, h = box[names[g.group]]
                        d = v.co - c
                        outside = outside and any(abs(d[k]) > h[k] + grow for k in range(3))
                n, bad = counts.get(seg, (0, 0))
                counts[seg] = (n + 1, bad + outside)
        out[piece] = {seg: bad / n for seg, (n, bad) in counts.items() if bad}
    return out


def through_by(pokes, verb="through"):
    """The points poking through (or in contact), by the layers: how many, how deep at worst,
    between which corners."""
    groups = {}
    for depth, outer, inner, p in pokes:
        groups.setdefault((inner, outer), []).append((depth, p))
    out = ""
    for (inner, outer), ps in sorted(groups.items(), key=lambda g: -max(g[1])[0]):
        lo = tuple(min(p[k] for _, p in ps) for k in range(3))
        hi = tuple(max(p[k] for _, p in ps) for k in range(3))
        out += f"; {inner} {verb} {outer} {len(ps)} (worst {1000 * max(ps)[0]:.1f} mm) in {lo}..{hi}"
    return out


def gaps(holes):
    """The uncovered points by piece and region: how many, and between which corners."""
    groups = {}
    for piece, region, p in holes:
        groups.setdefault((piece, region), []).append(p)
    out = ""
    for (piece, region), ps in sorted(groups.items(), key=lambda g: -len(g[1])):
        lo = tuple(min(p[k] for p in ps) for k in range(3))
        hi = tuple(max(p[k] for p in ps) for k in range(3))
        out += f"; {piece} {region} {len(ps)} in {lo}..{hi}"
    return out
