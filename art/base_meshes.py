"""Blender Studio's Human Base Meshes (v1.4.1, CC0; ADR-0026): the engineer's body is fitted from
their realistic man rather than modelled from nothing. The bundle (50 MB) is not committed: it
is downloaded once from its pinned address, checked against its pinned checksum, and kept under
`target/art-cache/`. Only its .blend is unpacked, and only mesh data is ever appended from it
(with no scripts run: it is someone else's file).
"""

import hashlib
import pathlib
import shutil
import urllib.request
import zipfile

URL = "https://mirror.blender.org/demo/asset-bundles/human-base-meshes/human-base-meshes-bundle-v1.4.1.zip"
SHA256 = "811f43accbb31a88266d932f8f5563b2d13586fca0ba2693aad1f5fe582b3515"
MEMBER = "human-base-meshes-bundle-v1.4.1/human_base_meshes_bundle.blend"
CACHE = pathlib.Path(__file__).resolve().parent.parent / "target" / "art-cache"


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def blend():
    """The bundle's .blend, downloading and checking it first if it is not cached."""
    out = CACHE / "human_base_meshes_bundle-v1.4.1.blend"
    if out.exists():
        return out
    CACHE.mkdir(parents=True, exist_ok=True)
    archive = CACHE / "human-base-meshes-bundle-v1.4.1.zip"
    if not archive.exists() or sha256(archive) != SHA256:
        print(f"art: downloading {URL}")
        # The mirror refuses Python's default user agent (403).
        request = urllib.request.Request(URL, headers={"User-Agent": "creeer-art/1"})
        with urllib.request.urlopen(request) as response, open(archive, "wb") as f:
            shutil.copyfileobj(response, f)
    if sha256(archive) != SHA256:
        archive.unlink()
        raise RuntimeError(f"{URL} is not the pinned bundle (sha256 differs); not used")
    with zipfile.ZipFile(archive) as z, z.open(MEMBER) as src, open(out, "wb") as dst:
        shutil.copyfileobj(src, dst)
    return out
