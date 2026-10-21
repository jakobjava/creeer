"""Builds one model headless (docs/blender-mcp.md): `cargo xtask art` runs

    blender --background --factory-startup --python art/build.py -- <name> <out.glb> [<out.blend>]

which runs art/<name>.py's build() in a clean Blender and exports the scene's meshes to
<out.glb>, and saves the scene to <out.blend> when asked. It prints the model's triangle count.
"""

import importlib
import pathlib
import sys

import bpy

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import common  # noqa: E402  (after the path is set)


def main():
    args = sys.argv[sys.argv.index("--") + 1 :]
    name, out = args[0], pathlib.Path(args[1])
    common.reset()
    model = importlib.import_module(name)
    obj = model.build()
    out.parent.mkdir(parents=True, exist_ok=True)
    common.export(out)
    print(f"art: {name}: {common.triangles(obj)} triangles -> {out}")
    if len(args) > 2:
        bpy.ops.wm.save_as_mainfile(filepath=str(pathlib.Path(args[2]).resolve()))


try:
    main()
except Exception:  # Blender would otherwise keep running with exit code 0
    import traceback

    traceback.print_exc()
    sys.exit(1)
