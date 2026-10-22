"""What the engineer's wearables hide when worn (`data/wearables.ron`, ADR-0026 decision 7), for
the scripts that check and show the model as the game draws it (`check_fit.py`,
`review_model.py`). The game reads the file itself (`creeer_art::wearable`)."""

import pathlib
import re

FILE = pathlib.Path(__file__).resolve().parent.parent.parent / "data" / "wearables.ron"


def hides():
    """The file read as {wearable: {piece: [regions]}} (its simple shape, by hand: no comments
    within an entry's lists)."""
    text = "\n".join(line.split("//")[0] for line in FILE.read_text(encoding="utf-8").splitlines())
    out = {}
    for chunk in text.split("(name: ")[1:]:
        name = re.match(r'"(\w+)"', chunk).group(1)
        out[name] = {p: re.findall(r'"([\w.]+)"', rs) for p, rs in re.findall(r'\("(\w+)", \[(.*?)\]\)', chunk, re.S)}
    if not out or not any(out.values()):
        raise SystemExit(f"art: read no wearables from {FILE}")
    return out


def hidden(worn, table=None):
    """{piece: regions} that the wearables `worn` hide, together."""
    out = {}
    for w, pieces in (table or hides()).items():
        if w in worn:
            for piece, regions in pieces.items():
                out.setdefault(piece, set()).update(regions)
    return out
