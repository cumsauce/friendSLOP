#!/usr/bin/env python3
"""Generates the modeler ops for the Ceiling Fan Museum map (`cfm-museum`).

Writes crates/game/.modeler/gen/fan_museum_ops.json: prefab instances (every
rotor with a `Spin`), `Block` collider markers, `Lamp`s, `room_*` boxes and the
`door` spot, in the museum's coordinates (origin = its floor centre, front door
at -z). Apply it with `apply_ops {doc: "cfm-museum", ops_file: ...}` and export
`maps/fan_museum.bsn`. After that the map is the truth: move things in the
modeler; re-run this only to regenerate the layout from scratch.
"""
import json
import math
import os
import random

CEILING = 8.0
# Skinned models (one sign, panel, placard, banner and poster with a skin per
# text) need a modeler_bevy that knows ModelerVariant; until the game pins
# one, place the separate per-text prefabs instead.
USE_SKINS = False
ops = []


def quat_mul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]


def quat_axis(axis, deg):
    h = math.radians(deg) / 2
    s = math.sin(h)
    return [axis[0] * s, axis[1] * s, axis[2] * s, math.cos(h)]


def rotate(q, v):
    x, y, z, w = quat_mul(quat_mul(q, [v[0], v[1], v[2], 0.0]), [-q[0], -q[1], -q[2], q[3]])
    return [x, y, z]


def quat_from_basis(right, up, fwd):
    # Columns: local x -> right, y -> up, z -> fwd.
    m00, m01, m02 = right[0], up[0], fwd[0]
    m10, m11, m12 = right[1], up[1], fwd[1]
    m20, m21, m22 = right[2], up[2], fwd[2]
    t = m00 + m11 + m22
    if t > 0:
        s = math.sqrt(t + 1.0) * 2
        return [(m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25 * s]
    if m00 > m11 and m00 > m22:
        s = math.sqrt(1.0 + m00 - m11 - m22) * 2
        return [0.25 * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s]
    if m11 > m22:
        s = math.sqrt(1.0 + m11 - m00 - m22) * 2
        return [(m01 + m10) / s, 0.25 * s, (m12 + m21) / s, (m02 - m20) / s]
    s = math.sqrt(1.0 + m22 - m00 - m11) * 2
    return [(m02 + m20) / s, (m12 + m21) / s, 0.25 * s, (m10 - m01) / s]


def norm(v):
    l = math.sqrt(sum(c * c for c in v)) or 1.0
    return [c / l for c in v]


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def r(v):
    return [round(c, 4) for c in v]


def prefab(name, pf, at, rot=None, rpm=None, skin=None):
    item = {"type": "prefab", "name": name, "prefab": pf, "translation": r(at)}
    if skin is not None:
        if USE_SKINS:
            item["variant"] = skin
        else:
            item["prefab"] = f"{pf}-{skin}"
    if rot is not None:
        item["rotation"] = r(rot)
    if rpm is not None:
        item["components"] = {"Spin": {"rpm": round(rpm, 2)}}
    ops.append(item)


def block(name, centre, size, round_=False, yaw=0.0):
    ops.append({
        "type": "marker", "name": name, "kind": "block", "translation": r(centre),
        "size": r(size), "yaw": yaw,
        "components": {"Block": {"round": round_}},
    })


def room(name, centre, size):
    ops.append({"type": "marker", "name": name, "kind": "zone", "translation": r(centre), "size": r(size)})


def lamp(name, at, lumens, rng):
    ops.append({
        "type": "marker", "name": name, "kind": "light", "translation": r(at),
        "components": {"Lamp": {"lumens": lumens, "range": rng}},
    })


# --- The building and the annex -------------------------------------------
prefab("building", "cfm-building-interior", [0, 0, 0])
prefab("annex", "cfm-annex-interior", [0, 0, 0])
room("room_museum", [0, 4, 0], [39.2, 8.5, 27.2])
room("room_grate", [-27.5, 6.5, 0], [14.6, 13.5, 27.2])
room("room_mobius", [-42.5, 6.5, 0], [14.6, 13.5, 27.2])
ops.append({"type": "marker", "name": "door", "kind": "spot", "translation": [0, 0.1, -14.15], "yaw": 0.0})

# Museum walls (the front with its door, back, east; the west wall has the
# annex door at z 3..7), roof and floor.
H = 9.0
block("wall_front_l", [-11, H / 2, -13.8], [18, H, 0.4])
block("wall_front_r", [11, H / 2, -13.8], [18, H, 0.4])
block("wall_front_top", [0, (3.5 + H) / 2, -13.8], [4, H - 3.5, 0.4])
block("wall_back", [0, H / 2, 13.8], [40, H, 0.4])
block("wall_east", [19.8, H / 2, 0], [0.4, H, 28])
block("wall_west_s", [-19.8, H / 2, -5.5], [0.4, H, 17])   # z -14..3
block("wall_west_n", [-19.8, H / 2, 10.5], [0.4, H, 7])    # z 7..14
block("wall_west_top", [-19.8, (3.5 + H) / 2, 5], [0.4, H - 3.5, 4])
block("roof", [0, 8.5, 0], [40, 1, 28])
block("floor", [0, -0.25, 0], [40, 0.5, 28])
# Furniture.
block("desk", [-12, 0.58, -10], [3.2, 1.16, 1.2])
block("gift_shelf", [-19.3, 1.0, -4], [0.6, 2, 6])
block("rope_line", [-7, 0.475, -11.5], [6.1, 0.95, 0.1])
for i, (x, z) in enumerate([(-18.2, 2), (-18.2, 8), (18.2, 2), (18.2, 8)]):
    block(f"sign_{i}", [x, 0.85, z], [0.15, 2, 1.4])
block("lamp_post_l", [-12, 3.5, -25.6], [0.24, 7, 0.24], True)
block("lamp_post_r", [12, 3.5, -25.6], [0.24, 7, 0.24], True)
for i, (x, z) in enumerate([(-12, -6), (0, -6), (12, -6), (-12, 6), (0, 8), (12, 6)]):
    lamp(f"lamp_{i}", [x, 7.6, z], 120000, 16)

# Annex: west, front and back walls, the partition with its door, roof, floor.
AH = 14.0
block("annex_west", [-49.8, AH / 2, 0], [0.4, AH, 28])
block("annex_front", [-35, AH / 2, -13.8], [30, AH, 0.4])
block("annex_back", [-35, AH / 2, 13.8], [30, AH, 0.4])
block("partition_s", [-35, AH / 2, -5.5], [0.4, AH, 17])
block("partition_n", [-35, AH / 2, 10.5], [0.4, AH, 7])
block("partition_top", [-35, (3.5 + AH) / 2, 5], [0.4, AH - 3.5, 4])
block("annex_roof", [-35, 13.5, 0], [30, 1, 28])
block("annex_floor", [-35, -0.25, 0], [30, 0.5, 28])
block("railing", [-27.5, 0.55, 9.5], [14.8, 1.1, 0.1])
for i, x in enumerate([-31, -24]):
    lamp(f"lamp_grate_{i}", [x, 12.5, 4], 200000, 20)
for i, z in enumerate([-7, 7]):
    lamp(f"lamp_mobius_{i}", [-42.5, 12.5, z], 200000, 20)

# --- The journey ---------------------------------------------------------
# Like a real museum's route: enter, turn right (most visitors do) into a
# chronological gallery, walk to the back where the Hall of Fame and a seated
# atrium are the magnet, come down the other side through the oddities (with
# the installations as a detour off it, the loudest last), and leave past the
# gift shop. Freestanding gallery walls (4.5 m, under the 8 m ceiling, so the
# fans overhead stay visible) make the loop; the atrium opens only to the back.
#
#   lobby (front) -> A Brief History (east, walking north) -> Hall of Fame and
#   the atrium (back, walking west) -> Cabinet of Curiosities (west, walking
#   south; the installations through the west door) -> gift shop & exit.
YAW90 = quat_axis([0, 1, 0], 90)
YAW_90 = quat_axis([0, 1, 0], -90)
YAW180 = quat_axis([0, 1, 0], 180)


def wall(name, a, b):
    """A gallery wall from a to b ([x, z]), 4.5 m tall and 0.3 thick."""
    length = math.hypot(b[0] - a[0], b[1] - a[1])
    mid = [(a[0] + b[0]) / 2, 0, (a[1] + b[1]) / 2]
    along_z = abs(b[1] - a[1]) > abs(b[0] - a[0])
    ops.append({"type": "prefab", "name": name, "prefab": "cfm-partition", "translation": r(mid),
                "rotation": r(YAW90 if along_z else [0, 0, 0, 1]), "scale": [round(length, 3), 1, 1]})
    size = [0.3, 4.5, length] if along_z else [length, 4.5, 0.3]
    block(f"{name}_box", [mid[0], 2.25, mid[2]], size)


wall("gallery_east", [6, -8], [6, 9])
wall("gallery_west", [-6, -8], [-6, 9])
wall("atrium_front", [-6, -8], [6, -8])


def sign(sid, at, rot=None):
    prefab(f"sign_{sid}", "cfm-sign", [at[0], CEILING, at[1]], rot, skin=sid)


sign("start", [3.5, -11])                 # over the lobby, pointing right
sign("history", [13, -7.6])               # the east gallery's way in
sign("fame", [7.5, 11], YAW90)            # across the back corridor
sign("curiosities", [-13, 9.3])           # the west gallery's way in
sign("exit", [-13, -8.4])                 # the way out, past the gift shelf


def bench(name, at, rot=None):
    prefab(name, "cfm-bench", [at[0], 0, at[1]], rot)
    block(f"{name}_box", [at[0], 0.23, at[1]], [2.0 if rot is None else 0.5, 0.46, 0.5 if rot is None else 2.0])


# 1. A Brief History of Moving Air: oldest first, walking north, a dated
#    panel on the gallery wall beside each fan.
HISTORY = [  # id, drop, rpm
    ("five", 0.5635, 60), ("propeller", 0.5, 140), ("eight", 0.44, -100),
    ("four", 0.344, -80), ("two", 0.43, 70), ("leaf", 0.375, 55),
    ("ten", 0.455, 65), ("caged", 0.35, 160), ("three", 0.287, 90),
]
for i, (fid, drop, rpm) in enumerate(HISTORY):
    z = -6.5 + i * 2.0
    x = 10.5 if i % 2 == 0 else 15.0
    prefab(f"fan_{fid}", f"cfm-fan-{fid}-mount", [x, CEILING, z])
    prefab(f"fan_{fid}_rotor", f"cfm-fan-{fid}-rotor", [x, CEILING - drop, z], rpm=rpm)
    prefab(f"timeline_{fid}", "cfm-timeline", [6.15, 1.0, z], YAW90, skin=fid)

# 2. The atrium: the industrial giant overhead, the Chandelier Supreme on its
#    dais, and benches facing the Wall of Fans.
prefab("fan_twelve", "cfm-fan-twelve-mount", [0, CEILING, -2.5])
prefab("fan_twelve_rotor", "cfm-fan-twelve-rotor", [0, CEILING - 2.0, -2.5], rpm=25)
bench("bench_0", [-3.2, 8.4])
bench("bench_1", [3.2, 8.4])
bench("bench_2", [-3.2, -6.4])
bench("bench_3", [3.2, -6.4])


def exhibit(fid, at, rpm, facing):
    """A weird fan low over its roped-off dais; its placard on the side
    visitors come from (`facing`: the way the placard reads, +x/-x/+z)."""
    x, z = at
    prefab(f"fan_{fid}", f"cfm-fan-{fid}-mount", [x, CEILING, z])
    prefab(f"fan_{fid}_rotor", f"cfm-fan-{fid}-rotor", [x, CEILING - 5.6, z], rpm=rpm)
    prefab(f"exhibit_{fid}", "cfm-exhibit", [x, 0, z])
    off, rot, yaw = {"+x": ([1.75, 0], YAW_90, -90.0), "-x": ([-1.75, 0], YAW90, 90.0),
                     "+z": ([0, 1.75], YAW180, 180.0)}[facing]
    px, pz = x + off[0], z + off[1]
    prefab(f"placard_{fid}", "cfm-placard", [px, 0, pz], rot, skin=fid)
    block(f"cordon_{fid}", [x, 0.575, z], [3.2, 1.15, 3.2])
    block(f"placard_{fid}_stand", [px, 0.5, pz], [0.6, 1.0, 0.3], yaw=yaw)


exhibit("chandelier", (0, 5.0), 20, "+z")

# 3. Cabinet of Curiosities: two rows along a central aisle, walking south,
#    the placards facing the aisle; the installations through the west door.
exhibit("spoons", (-9.0, 6.0), 45, "-x")
exhibit("hands", (-9.0, 0.5), -40, "-x")
exhibit("ring", (-9.0, -5.0), 50, "-x")
exhibit("vinyl", (-16.5, 0.0), 33.3, "+x")
exhibit("tentacle", (-16.5, -5.0), 30, "+x")

# --- Dressing: trim, columns, light, banners, plants, cases, posters ------
def piece(name, pf, at, yaw=0.0, length=None, axis="x", skin=None):
    item = {"type": "prefab", "name": name, "prefab": pf, "translation": r(at)}
    if skin is not None:
        if USE_SKINS:
            item["variant"] = skin
        else:
            item["prefab"] = f"{pf}-{skin}"
    if yaw:
        item["rotation"] = r(quat_axis([0, 1, 0], yaw))
    if length is not None:
        item["scale"] = [round(length, 3), 1, 1] if axis == "x" else [1, 1, round(length, 3)]
    ops.append(item)


# Crown moulding under the ceiling and baseboards, on the inside of the outer
# walls (the strips stick out toward -z; yaw turns them to face the room).
IN = 19.6, 13.6
for kind, y in (("crown", CEILING), ("base", 0.0)):
    piece(f"{kind}_back", f"cfm-trim-{kind}", [0, y, IN[1]], 0, 39.2)
    piece(f"{kind}_east", f"cfm-trim-{kind}", [IN[0], y, 0], 90, 27.2)
    if kind == "crown":
        piece("crown_front", "cfm-trim-crown", [0, y, -IN[1]], 180, 39.2)
        piece("crown_west", "cfm-trim-crown", [-IN[0], y, 0], -90, 27.2)
    else:  # the west wall has the installations door at z 3..7
        piece("base_west_s", "cfm-trim-base", [-IN[0], y, -5.3], -90, 16.6)
        piece("base_west_n", "cfm-trim-base", [-IN[0], y, 10.3], -90, 6.6)
# Pilasters along the side and back walls.
for i, z in enumerate((-8.0, -2.0, 4.0, 10.5)):
    piece(f"pilaster_e{i}", "cfm-pilaster", [IN[0], 0, z], 90)
for i, z in enumerate((-11.0, 9.0, 12.0)):
    piece(f"pilaster_w{i}", "cfm-pilaster", [-IN[0], 0, z], -90)
for i, x in enumerate((-18.5, -14.0, 14.0, 18.5)):
    piece(f"pilaster_b{i}", "cfm-pilaster", [x, 0, IN[1]], 0)

# Track lights: over the history panels, the curiosities aisle and the Wall of
# Fans, each with a warm lamp under it.
TRACKS = [("hist0", 8.5, -4.0, 90), ("hist1", 8.5, 2.0, 90), ("hist2", 8.5, 8.0, 90),
          ("cur0", -12.75, 3.5, 90), ("cur1", -12.75, -3.5, 90),
          ("fame0", -6.0, 11.2, 0), ("fame1", 0.0, 11.2, 0), ("fame2", 6.0, 11.2, 0)]
for tid, x, z, yaw in TRACKS:
    piece(f"track_{tid}", "cfm-track-light", [x, CEILING, z], yaw)
    lamp(f"lamp_track_{tid}", [x, CEILING - 0.6, z], 40000, 9)

# Banners in the lobby and the history gallery's far corner.
piece("banner_breeze", "cfm-banner", skin="breeze", at=[-8.0, CEILING, -12.8])
piece("banner_since", "cfm-banner", skin="since", at=[8.0, CEILING, -12.8])
piece("banner_spin", "cfm-banner", skin="spin", at=[18.9, CEILING, 11.5], yaw=90)

# Plants in the lobby corners, either side of the atrium, the back corners.
for i, (x, z) in enumerate([(-18.5, -12.5), (18.5, -12.5), (-5.0, -7.0), (5.0, -7.0), (-18.6, 12.6), (18.6, 12.6)]):
    piece(f"plant_{i}", "cfm-plant", [x, 0, z])
    block(f"plant_{i}_box", [x, 0.4, z], [0.6, 0.8, 0.6], True)

# Carpet runners down both galleries' walking lines.
piece("runner_history", "cfm-runner", [12.75, 0.005, 0.5], 0, 17.0, "z")
piece("runner_curiosities", "cfm-runner", [-12.75, 0.005, 0.5], 0, 17.0, "z")

# Vintage posters in the atrium, on the gallery walls' atrium faces.
piece("poster_breezeway", "cfm-poster", skin="breezeway", at=[-5.85, 1.3, 0.5], yaw=-90)
piece("poster_whirlwind", "cfm-poster", skin="whirlwind", at=[5.85, 1.3, 0.5], yaw=90)
piece("poster_aerodyne", "cfm-poster", skin="aerodyne", at=[0.0, 1.3, -7.85], yaw=180)

# Glass cases: the oldest things in the history gallery along its outer wall
# (between the info stands), the rest in the atrium facing the Hall of Fame.
CASES = [("motor", 18.7, -6.0, 90), ("patent", 18.7, -3.5, 90), ("blades", 18.7, 5.0, 90),
         ("chains", -4.2, 0.5, 180), ("remote", 4.2, 0.5, 180)]
for cid, x, z, yaw in CASES:
    piece(f"case_{cid}", f"cfm-vitrine-{cid}", [x, 0, z], yaw)
    size = [0.8, 1.5, 1.0] if yaw in (90, -90) else [1.0, 1.5, 0.8]
    block(f"case_{cid}_box", [x, 0.75, z], size)

# --- The Wall of Fans --------------------------------------------------------
WALL = [0, 0, 13.6]
prefab("celeb_wall", "cfm-celeb-wall-static", WALL)
block("celeb_wall_box", [0, 2.5, 13.25], [22.2, 5.0, 0.7])
OUT = quat_axis([1, 0, 0], -90)  # rotor +Y -> -Z, out of the wall
hubs = [(x, y) for y in (2.95, 1.1) for x in (-7.4, -3.0, 1.4, 5.8, 10.2)]
for i, (x, y) in enumerate(hubs):
    rpm = 80 * (0.7 + 0.1 * (i % 7)) * (1 if i % 2 == 0 else -1)
    prefab(f"celeb_rotor_{i}", f"cfm-celeb-rotor-{i}", [WALL[0] + x, WALL[1] + y, WALL[2] - 0.6], OUT, rpm)

# --- Hall A: the Grate One ---------------------------------------------------
GIANT = [-27.5, 0, 13.6]
prefab("giant", "cfm-giant-mount", GIANT)
prefab("giant_rotor", "cfm-giant-rotor", [GIANT[0], 6.25, GIANT[2] - 0.9], OUT, 14)
block("giant_housing", [GIANT[0], 6.25, GIANT[2] - 0.95], [12.5, 12.5, 1.9])

# --- Hall B: the Möbius of desk fans -----------------------------------------
MOB = [-42.5, 0, 0]
R, HALF_W, UP = 4.5, 1.2, 6.0
prefab("mobius", "cfm-mobius", MOB)
block("mobius_pylon_0", [MOB[0] + 4.5, 3.0, MOB[2]], [1.2, 6.0, 1.2], True)
block("mobius_pylon_1", [MOB[0] - 4.5, 2.4, MOB[2]], [1.2, 4.8, 1.2], True)


def strip(u, v):
    c = math.cos(u / 2)
    return [(R + v * c) * math.cos(u), UP + v * math.sin(u / 2), (R + v * c) * math.sin(u)]


def normal(u, v):
    e = 1e-3
    du = [(a - b) / (2 * e) for a, b in zip(strip(u + e, v), strip(u - e, v))]
    dv = [(a - b) / (2 * e) for a, b in zip(strip(u, v + e), strip(u, v - e))]
    return norm(cross(du, dv)), norm(du)


HUB = [0, 0.28, 0.03]
SIT = 0.045  # half the strip's thickness, plus a hair
rng = random.Random(7)
n = 0
STEPS = 72
for k in range(STEPS * 2):  # two laps of u: both faces of the one-sided strip
    u = (k + 0.5) * 2 * math.pi / STEPS
    for v in (-0.6, 0.6):
        nrm, tan = normal(u, v)
        p = strip(u, v)
        # Keep the pylons' feet clear.
        if nrm[1] < -0.3 and math.cos(u) > 0.96:
            continue
        fwd = norm([t - nrm[i] * sum(a * b for a, b in zip(tan, nrm)) for i, t in enumerate(tan)])
        right = cross(nrm, fwd)
        q = quat_from_basis(right, nrm, fwd)
        base = [MOB[0] + p[0] + nrm[0] * SIT, MOB[1] + p[1] + nrm[1] * SIT, MOB[2] + p[2] + nrm[2] * SIT]
        hub = rotate(q, HUB)
        rpm = rng.uniform(140, 260) * rng.choice([1, 1, 1, -1])
        prefab(f"desk_{n}", "cfm-desk-lo-mount", base, q)
        prefab(f"desk_{n}_rotor", "cfm-desk-lo-rotor", [base[i] + hub[i] for i in range(3)],
               quat_mul(q, quat_axis([1, 0, 0], 90)), rpm)
        n += 1

out = os.path.join(os.path.dirname(__file__), "..", "crates", "game", ".modeler", "gen", "fan_museum_ops.json")
os.makedirs(os.path.dirname(out), exist_ok=True)
with open(out, "w") as f:
    json.dump(ops, f)
print(f"{len(ops)} ops, {n} desk fans -> {os.path.normpath(out)}")
