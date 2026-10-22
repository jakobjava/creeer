"""The engineer (G1.3f; ADR-0026): the base body and the starting outfit's wearables, in three
levels of detail, divided into regions and skinned on the ragdoll's fifteen segments. Built by
`art/engineer/models.py` from Blender Studio's base mesh fitted to measured men; `cargo xtask art
engineer` writes `data/models/engineer.glb`.
"""

import importlib
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent / "engineer"))
import models  # noqa: E402  (after the path is set)

models = importlib.reload(models)


def build():
    parts = models.build()
    return max(parts, key=models.triangles)
