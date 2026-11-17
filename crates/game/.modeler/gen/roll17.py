import json, math

REVS, DUR = 3, 1.8
K = 36 * REVS
A, B = 0.885, 0.935
H = (0.0, 1.998, -0.009)
HIP0 = (0.0, 0.558, -0.529)
CENT = (0.0, 0.609, -0.126)
V = (0.0, CENT[1] - HIP0[1], CENT[2] - HIP0[2])
U = [((0.0, 0.0, 0.0), 0.10),
     ((0.149, 0.030 - CENT[1], -0.531 - CENT[2]), 0.05),
     ((-0.151, -0.020 - CENT[1], -0.061 - CENT[2]), 0.05),
     ((0.389, 0.264 - CENT[1], 0.105 - CENT[2]), 0.02),
     ((-0.378, 0.377 - CENT[1], -0.213 - CENT[2]), 0.02),
     ((0.016, 0.520 - CENT[1], 0.218 - CENT[2]), 0.19)]

def rot(y, z, deg):
    t = math.radians(deg)
    return y * math.cos(t) - z * math.sin(t), y * math.sin(t) + z * math.cos(t)

def hip_pose(deg):
    h = -min(rot(u[1], u[2], deg)[0] - r for u, r in U)
    ry, rz = rot(V[1], V[2], deg)
    return h - ry, -rz          # hip (y, z) in world

def leg_solve(deg, u):
    """u = 0..1 across the planted window. Returns (thigh, knee, ankle) local."""
    hy, hz = hip_pose(deg)
    foot_z = hz + 0.40 - 0.80 * u
    foot_y = 0.06
    dy, dz = hy - foot_y, foot_z - hz
    d = min(A + B - 0.02, max(abs(A - B) + 0.02, math.hypot(dy, dz)))
    ca = (A * A + d * d - B * B) / (2 * A * d)
    alpha = math.degrees(math.acos(max(-1.0, min(1.0, ca))))
    ck = (A * A + B * B - d * d) / (2 * A * B)
    knee = 180.0 - math.degrees(math.acos(max(-1.0, min(1.0, ck))))
    thigh_dir = math.degrees(math.atan2(dz, dy)) + alpha
    thigh = -thigh_dir - deg
    ankle = -(deg + thigh + knee) * 0.9
    return thigh, knee, ankle

base = {
    "thigh_l": -132, "thigh_r": -132, "knee_l": 162, "knee_r": 162,
    "ankle_l": 30, "ankle_r": 30, "shoulder_l": -84, "shoulder_r": -84,
    "elbow_l": -126, "elbow_r": -126, "wrist_l": -14, "wrist_r": -14,
    "hand_l": 162, "hand_r": 162, "spine": 52, "chest": 46,
    "neck": 30, "head": 38, "jaw": 14,
}
react = {
    "shoulder_l": 52, "shoulder_r": 52, "elbow_l": 58, "elbow_r": 58,
    "wrist_l": 55, "wrist_r": 55, "hand_l": 30, "hand_r": 30,
    "spine": 7, "chest": 7, "neck": 9, "head": 16, "jaw": 14,
}
limit = {"elbow_l": (-176, -34), "elbow_r": (-176, -34),
         "shoulder_l": (-142, -18), "shoulder_r": (-142, -18)}
LIMB = {"shoulder_l": (0.31, 0.116), "shoulder_r": (-0.31, 0.116),
        "elbow_l": (-0.04, -0.056), "elbow_r": (-0.04, -0.056),
        "wrist_l": (0.12, 0.216), "wrist_r": (0.12, 0.216),
        "hand_l": (0.12, 0.216), "hand_r": (0.12, 0.216),
        "spine": (0.18, 0.24), "chest": (0.31, 0.30), "neck": (0.23, 0.20),
        "head": (-0.09, 0.346), "jaw": (-0.09, 0.30)}
lag = {"shoulder_l": 8, "shoulder_r": 8, "elbow_l": 14, "elbow_r": 14,
       "wrist_l": 22, "wrist_r": 22, "hand_l": 26, "hand_r": 26,
       "spine": 4, "chest": 6, "neck": 8, "head": 12, "jaw": 16}

def smooth(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3.0 - 2.0 * t)

def drive(bone, deg):
    """Smooth, saturating: a limp limb swings, it doesn't flick."""
    dy, dz = LIMB[bone]
    r = math.hypot(dy, dz) or 1.0
    up = rot(dy, dz, -(deg - lag[bone]))[0] / r
    return math.tanh(1.7 * up)

def plant(deg, phase):
    """How much of the leg's weight is on the floor. It drags for a third of
    the turn, then peels up and reaches back down over the rest: no corners."""
    a = (deg + phase) % 360.0
    if a < 120.0:                       # planted, dragging under the body
        return 1.0, a / 120.0
    if a < 240.0:                       # peeling up into the tuck
        return 1.0 - smooth((a - 120.0) / 120.0), 1.0
    # swinging forward through the air, reaching the floor again ahead
    return smooth((a - 240.0) / 120.0), 1.0 - smooth((a - 240.0) / 120.0)

def mix(a, b, f):
    return a * (1.0 - f) + b * f

tracks, legs = {}, {}
for i in range(K + 1):
    t = i / K
    deg = 360.0 * (t * REVS % 1.0)
    for side, phase in (("l", 0.0), ("r", 180.0)):
        hold, u = plant(deg, phase)
        lt, lk, la = leg_solve(deg, u)
        tt = mix(base[f"thigh_{side}"], lt, hold)
        kk = mix(base[f"knee_{side}"], lk, hold)
        aa = mix(base[f"ankle_{side}"], la, hold)
        if hold < 0.5:                  # loose while it's in the air
            d = drive(f"thigh_{side}", deg) if f"thigh_{side}" in LIMB else 0.0
        else:
            d = 0.0
        legs.setdefault(f"thigh_{side}", []).append(
            [round(t * DUR, 3), {"x": round(tt, 1), "z": round(6.0 * d, 1)}])
        legs.setdefault(f"knee_{side}", []).append(
            [round(t * DUR, 3), {"x": round(kk, 1)}])
        legs.setdefault(f"ankle_{side}", []).append(
            [round(t * DUR, 3), {"x": round(aa, 1)}])

for bone, x0 in base.items():
    if bone.startswith(("thigh", "knee", "ankle")):
        continue
    keys = []
    for i in range(K + 1):
        t = i / K
        rev = t * REVS
        deg = 360.0 * (rev % 1.0)
        breathe = 1.0 + 0.5 * math.sin(2 * math.pi * (rev / REVS) + (len(bone) % 7))
        d = drive(bone, deg) * breathe
        x = x0 + react[bone] * d
        if bone in limit:
            x = max(limit[bone][0], min(limit[bone][1], x))
        keys.append([round(t * DUR, 3), {"x": round(x, 1),
                                         "z": round(react[bone] * 0.45 * d, 1)}])
    tracks[bone] = {"rotate": keys}

trans = []
for i in range(K + 1):
    deg = 360.0 * (i / K * REVS % 1.0)
    hy, hz = hip_pose(deg)
    trans.append([round(i / K * DUR, 3), [0.0, round(hy - H[1], 3), round(hz - H[2], 3)]])
tracks["hip"] = {"translate": trans,
                 "rotate": [[round(i / (12 * REVS) * DUR, 3), {"x": i * 30}]
                            for i in range(12 * REVS + 1)]}
for bone, keys in legs.items():
    tracks[bone] = {"rotate": keys}
op = {"type": "clip", "name": "roll", "skeleton": "hunter_rig", "duration": DUR,
      "loop": True, "interp": "smooth", "tracks": tracks}
json.dump([op], open("crates/game/.modeler/gen/roll17.json", "w"), indent=1)
print("thigh_l", [k[1]["x"] for k in tracks["thigh_l"]["rotate"]][:14])
print("knee_l ", [k[1]["x"] for k in tracks["knee_l"]["rotate"]][:14])
