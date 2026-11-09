#!/usr/bin/env python3
"""The bot's Discord profile picture, drawn from the game's own models (#170).

A tiny software renderer: it reads the game's CC0 glTF models (the campfire and the cowboy
characters, all flat-coloured), poses them around the fire at night, flat-shades every triangle
with Pillow and stamps the current status on top in the game's Sniglet font:

  - the big number is how many ideas have landed in the game,
  - one cowboy sits at the fire per idea being built right now (at least one keeps watch),
  - the fire grows with the number of ideas waiting in the queue,
  - a red ring means an idea is stuck and needs a human.

If any asset is missing (LFS quota, network) it quietly draws flat primitives instead.

Try it locally: `python3 avatar.py out.png [landed] [building] [waiting] [stuck]`
(fetches the models from GitHub).
"""

from __future__ import annotations

import io
import json
import math
import struct
from dataclasses import dataclass

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

SIZE = 512
SUPERSAMPLE = 2
# Paths under crates/game/assets/ (served by GitHub's LFS media host).
ASSETS = {
    "male": "models/characters/Cowboy_Male.glb",
    "female": "models/characters/Cowboy_Female.glb",
    "fire": "models/props/bonfire_fire.glb",
    "font": "fonts/Sniglet-ExtraBold.ttf",
}
SKY_TOP = (14, 18, 46)
SKY_BOTTOM = (52, 34, 70)
GROUND = (34, 52, 30)
STUCK_RED = (220, 50, 50)
FIRE_GLOW = np.array([1.0, 0.55, 0.2])


@dataclass
class Status:
    landed: int = 0
    building: int = 0
    waiting: int = 0
    stuck: int = 0


Triangles = list[tuple[np.ndarray, tuple[float, float, float]]]  # (3x3 verts, linear rgb)


# --- glTF ------------------------------------------------------------------------------------


def _glb(data: bytes) -> tuple[dict, bytes]:
    if data[:4] != b"glTF":
        raise ValueError("not a glb")
    json_len = struct.unpack_from("<I", data, 12)[0]
    gltf = json.loads(data[20 : 20 + json_len])
    rest = 20 + json_len
    bin_len = struct.unpack_from("<I", data, rest)[0]
    return gltf, data[rest + 8 : rest + 8 + bin_len]


_COMPONENTS = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
_WIDTH = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


def _accessor(gltf: dict, buf: bytes, index: int) -> np.ndarray:
    acc = gltf["accessors"][index]
    view = gltf["bufferViews"][acc["bufferView"]]
    dtype = np.dtype(_COMPONENTS[acc["componentType"]])
    width = _WIDTH[acc["type"]]
    start = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
    stride = view.get("byteStride") or dtype.itemsize * width
    count = acc["count"]
    raw = np.frombuffer(buf, dtype=np.uint8, count=stride * (count - 1) + dtype.itemsize * width, offset=start)
    rows = np.lib.stride_tricks.as_strided(raw, shape=(count, dtype.itemsize * width), strides=(stride, 1))
    out = np.ascontiguousarray(rows).view(dtype).reshape(count, width).astype(np.float64)
    if acc.get("normalized"):
        out /= np.iinfo(dtype).max
    return out


def _local(node: dict) -> np.ndarray:
    if "matrix" in node:
        return np.array(node["matrix"], dtype=np.float64).reshape(4, 4).T
    x, y, z, w = node.get("rotation", [0, 0, 0, 1])
    rot = np.array(
        [
            [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
            [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
            [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
        ]
    )
    m = np.eye(4)
    m[:3, :3] = rot * np.array(node.get("scale", [1, 1, 1]))
    m[:3, 3] = node.get("translation", [0, 0, 0])
    return m


def _world_matrices(gltf: dict) -> dict[int, np.ndarray]:
    nodes = gltf["nodes"]
    world: dict[int, np.ndarray] = {}
    roots = gltf["scenes"][gltf.get("scene", 0)]["nodes"]
    stack = [(i, np.eye(4)) for i in roots]
    while stack:
        i, parent = stack.pop()
        world[i] = parent @ _local(nodes[i])
        stack += [(c, world[i]) for c in nodes[i].get("children", [])]
    return world


def _srgb(c: float) -> float:
    return c * 12.92 if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055


def load_model(data: bytes) -> Triangles:
    """Every triangle of a glb's default scene in its rest pose, with its material colour."""
    gltf, buf = _glb(data)
    world = _world_matrices(gltf)
    tris: Triangles = []
    for ni, node in enumerate(gltf["nodes"]):
        if "mesh" not in node or ni not in world:
            continue
        skin = gltf["skins"][node["skin"]] if "skin" in node else None
        joint_mats = None
        if skin is not None:
            ibm = _accessor(gltf, buf, skin["inverseBindMatrices"]).reshape(-1, 4, 4).transpose(0, 2, 1)
            joint_mats = np.stack([world.get(j, np.eye(4)) @ ibm[k] for k, j in enumerate(skin["joints"])])
        for prim in gltf["meshes"][node["mesh"]]["primitives"]:
            if prim.get("mode", 4) != 4:
                continue
            pos = _accessor(gltf, buf, prim["attributes"]["POSITION"])
            homo = np.hstack([pos, np.ones((len(pos), 1))])
            if joint_mats is not None and "JOINTS_0" in prim["attributes"]:
                joints = _accessor(gltf, buf, prim["attributes"]["JOINTS_0"]).astype(int)
                weights = _accessor(gltf, buf, prim["attributes"]["WEIGHTS_0"])
                skinned = np.einsum("vk,vkij->vij", weights, joint_mats[joints])
                verts = np.einsum("vij,vj->vi", skinned, homo)[:, :3]
            else:
                verts = (homo @ world[ni].T)[:, :3]
            if "indices" in prim:
                idx = _accessor(gltf, buf, prim["indices"]).astype(int).reshape(-1, 3)
            else:
                idx = np.arange(len(verts) - len(verts) % 3).reshape(-1, 3)
            mat = gltf.get("materials", [{}])[prim.get("material", 0)] if gltf.get("materials") else {}
            r, g, b, _ = mat.get("pbrMetallicRoughness", {}).get("baseColorFactor", [0.8, 0.8, 0.8, 1])
            emissive = mat.get("emissiveFactor", [0, 0, 0])
            colour = (max(r, emissive[0]), max(g, emissive[1]), max(b, emissive[2]))
            tris += [(verts[t], colour) for t in idx]
    return tris


def fit(tris: Triangles, height: float, at: tuple[float, float, float], yaw: float = 0.0) -> Triangles:
    """Scale a model to `height` (standing on y=0), turn it by `yaw` and move it to `at`."""
    allv = np.concatenate([t for t, _ in tris])
    lo, hi = allv.min(axis=0), allv.max(axis=0)
    scale = height / max(hi[1] - lo[1], 1e-6)
    centre = np.array([(lo[0] + hi[0]) / 2, lo[1], (lo[2] + hi[2]) / 2])
    c, s = math.cos(yaw), math.sin(yaw)
    rot = np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]])
    offset = np.array(at)
    return [(((v - centre) * scale) @ rot.T + offset, col) for v, col in tris]


# --- rendering -------------------------------------------------------------------------------


class Camera:
    def __init__(self, eye, target, fov_deg: float, size: int) -> None:
        self.eye = np.array(eye, dtype=np.float64)
        fwd = np.array(target) - self.eye
        fwd /= np.linalg.norm(fwd)
        right = np.cross(fwd, [0, 1, 0])
        right /= np.linalg.norm(right)
        up = np.cross(right, fwd)
        self.basis = np.stack([right, up, fwd])
        self.focal = size / 2 / math.tan(math.radians(fov_deg) / 2)
        self.size = size

    def project(self, pts: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
        cam = (pts - self.eye) @ self.basis.T
        z = np.maximum(cam[..., 2], 1e-3)
        x = self.size / 2 + cam[..., 0] / z * self.focal
        y = self.size / 2 - cam[..., 1] / z * self.focal
        return np.stack([x, y], axis=-1), cam[..., 2]


def _shade(tri: np.ndarray, colour, fire_at: np.ndarray, glow: bool) -> tuple[int, int, int]:
    normal = np.cross(tri[1] - tri[0], tri[2] - tri[0])
    n = np.linalg.norm(normal)
    normal = normal / n if n > 0 else np.array([0, 1, 0])
    centre = tri.mean(axis=0)
    to_fire = fire_at - centre
    dist = np.linalg.norm(to_fire)
    fire_light = abs(float(normal @ (to_fire / max(dist, 1e-6)))) * 2.2 / (1 + dist * dist * 0.5)
    moon = abs(float(normal @ np.array([-0.4, 0.8, 0.45]))) * 0.35
    base = np.array(colour)
    lit = base * (0.12 + moon) + base * FIRE_GLOW * fire_light
    if glow:
        lit = base * 2.2 + np.array([0.15, 0.05, 0.0])
    return tuple(int(round(255 * _srgb(float(min(max(c, 0.0), 1.0))))) for c in lit)


def _backdrop(size: int) -> Image.Image:
    img = Image.new("RGB", (size, size))
    draw = ImageDraw.Draw(img)
    for y in range(size):
        t = y / size
        draw.line(
            [(0, y), (size, y)],
            fill=tuple(int(a + (b - a) * t) for a, b in zip(SKY_TOP, SKY_BOTTOM)),
        )
    rng = np.random.default_rng(170)
    for x, y, r in zip(rng.uniform(0, size, 40), rng.uniform(0, size * 0.45, 40), rng.uniform(1, 3, 40)):
        draw.ellipse([x - r, y - r, x + r, y + r], fill=(235, 235, 210))
    horizon = int(size * 0.55)
    draw.rectangle([0, horizon, size, size], fill=GROUND)
    return img


def _glow(img: Image.Image, centre: tuple[float, float], radius: float) -> Image.Image:
    glow = Image.new("RGB", img.size, (0, 0, 0))
    ImageDraw.Draw(glow).ellipse(
        [centre[0] - radius, centre[1] - radius, centre[0] + radius, centre[1] + radius],
        fill=(150, 70, 20),
    )
    glow = glow.filter(ImageFilter.GaussianBlur(radius / 2.5))
    return Image.fromarray(np.minimum(np.asarray(img, dtype=np.int32) + np.asarray(glow), 255).astype(np.uint8))


def scene(models: dict[str, Triangles], status: Status) -> list[tuple[Triangles, bool]]:
    """The campfire with one cowboy per idea being built, sitting in an arc behind it."""
    fire_height = 0.7 + 0.12 * min(status.waiting, 6)
    parts: list[tuple[Triangles, bool]] = []
    if "fire" in models:
        parts.append((fit(models["fire"], fire_height, (0, 0, 0)), False))
    people = max(1, min(status.building, 5))
    for i in range(people):
        angle = 0.0 if people == 1 else -0.9 + 1.8 * i / (people - 1)
        x, z = math.sin(angle) * 1.5, math.cos(angle) * 1.5
        body = models.get("female" if i % 2 else "male") or models.get("male") or models.get("female")
        if body:
            # Characters face +z; turn each one to face the fire (and so roughly the camera).
            parts.append((fit(body, 1.6, (x, 0, z), math.pi + angle), False))
    return parts


def _fallback(draw: ImageDraw.ImageDraw, size: int, status: Status) -> None:
    """Flat stand-ins if the models couldn't be fetched: a fire and some round heads."""
    u = size / 512
    cx, base = size / 2, size * 0.66
    people = max(1, min(status.building, 5))
    for i in range(people):
        x = cx + (i - (people - 1) / 2) * size * 0.16
        draw.rounded_rectangle([x - 26 * u, base - 150 * u, x + 26 * u, base - 40 * u], 18 * u, fill=(110, 60, 30))
        draw.ellipse([x - 22 * u, base - 196 * u, x + 22 * u, base - 152 * u], fill=(232, 190, 150))
    h = (60 + 10 * min(status.waiting, 6)) * u
    floor = base + 20 * u
    draw.polygon([(cx - 45 * u, floor), (cx + 45 * u, floor), (cx, floor - h * 1.6)], fill=(255, 140, 30))
    draw.polygon([(cx - 22 * u, floor), (cx + 22 * u, floor), (cx, floor - h)], fill=(255, 220, 90))


def _label(img: Image.Image, font_data: bytes | None, status: Status) -> None:
    size = img.size[0]
    draw = ImageDraw.Draw(img)
    text = str(status.landed)

    def font(px: int):
        if font_data:
            try:
                return ImageFont.truetype(io.BytesIO(font_data), px)
            except OSError:
                pass
        return ImageFont.load_default(px)

    big = font(int(size * (0.3 if len(text) < 3 else 0.24)))
    small = font(int(size * 0.07))
    cx, y = size / 2, size * 0.83
    draw.text((cx, y), text, font=big, anchor="mm", fill=(255, 236, 170), stroke_width=size // 64, stroke_fill=(40, 20, 10))
    draw.text((cx, size * 0.13), "friendSLOP", font=small, anchor="mm", fill=(255, 255, 255), stroke_width=size // 160, stroke_fill=(20, 20, 40))


def render(assets: dict[str, bytes], status: Status) -> bytes:
    """PNG bytes of the avatar for `status`, using whichever `assets` (see ASSETS) loaded."""
    big = SIZE * SUPERSAMPLE
    img = _backdrop(big)
    models: dict[str, Triangles] = {}
    for key in ("male", "female", "fire"):
        try:
            if assets.get(key):
                models[key] = load_model(assets[key])
        except Exception:  # noqa: BLE001 - a broken model just isn't drawn
            pass
    cam = Camera((0, 1.9, -3.7), (0, 0.95, 0.7), 50, big)
    fire_at = np.array([0, 0.5, 0])
    fire_px, _ = cam.project(fire_at[None])
    img = _glow(img, (fire_px[0][0], fire_px[0][1]), big * 0.28)
    draw = ImageDraw.Draw(img)
    if "fire" not in models or not ("male" in models or "female" in models):
        _fallback(draw, big, status)
    else:
        polys = []
        for tris, _ in scene(models, status):
            for verts, colour in tris:
                pts, depth = cam.project(verts)
                if (depth <= 0.05).any():
                    continue
                glow = colour[0] > 0.3 and colour[2] < 0.05 and colour[0] > colour[1] * 2.5
                polys.append((float(depth.mean()), pts, _shade(verts, colour, fire_at, glow)))
        polys.sort(key=lambda p: -p[0])
        for _, pts, fill in polys:
            draw.polygon([tuple(p) for p in pts], fill=fill)
    _label(img, assets.get("font"), status)
    if status.stuck:
        ImageDraw.Draw(img).ellipse([8, 8, big - 8, big - 8], outline=STUCK_RED, width=big // 28)
    out = io.BytesIO()
    img.resize((SIZE, SIZE), Image.LANCZOS).save(out, "PNG", optimize=True)
    return out.getvalue()


def main() -> None:
    import sys
    import urllib.request

    args = sys.argv[1:]
    path = args[0] if args else "avatar.png"
    nums = [int(a) for a in args[1:5]]
    base = "https://media.githubusercontent.com/media/cumsauce/friendSLOP/main/crates/game/assets/"
    assets = {}
    for key, rel in ASSETS.items():
        try:
            with urllib.request.urlopen(base + rel, timeout=30) as resp:
                assets[key] = resp.read()
        except OSError as e:
            print(f"skipping {rel}: {e}")
    with open(path, "wb") as f:
        f.write(render(assets, Status(*nums)))
    print(f"wrote {path}")


if __name__ == "__main__":
    main()
