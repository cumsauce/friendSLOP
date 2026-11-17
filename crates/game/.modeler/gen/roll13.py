import json, math

REVS = 3
DUR, K = 0.6 * REVS, 36 * REVS
H = (0.0, 1.998, -0.009)
HIP0 = (0.0, 0.558, -0.529)
CENT = (0.0, 0.609, -0.126)

# each limb's rest offset from the ball's centre in the (y, z) plane, so we
# know where it is in the spin, plus how hard it reacts and how late
LIMB = {
    "shoulder_l": (0.31, 0.116), "shoulder_r": (-0.31, 0.116),
    "elbow_l":    (-0.04, -0.056), "elbow_r": (-0.04, -0.056),
    "wrist_l":    (0.12, 0.216),  "wrist_r": (0.12, 0.216),
    "hand_l":     (0.12, 0.216),  "hand_r":  (0.12, 0.216),
    "thigh_l":    (-0.049, -0.404), "thigh_r": (-0.049, -0.404),
    "knee_l":     (0.44, 0.296),  "knee_r":  (0.44, 0.296),
    "ankle_l":    (-0.20, -0.386), "ankle_r": (-0.20, -0.386),
    "spine":      (0.18, 0.24), "chest": (0.31, 0.30), "neck": (0.23, 0.20),
    "head":       (-0.09, 0.346), "jaw": (-0.09, 0.30),
}
base = {
    "thigh_l": -132, "thigh_r": -132, "knee_l": 162, "knee_r": 162,
    "ankle_l": 30, "ankle_r": 30, "shoulder_l": -84, "shoulder_r": -84,
    "elbow_l": -126, "elbow_r": -126, "wrist_l": -14, "wrist_r": -14,
    "hand_l": 162, "hand_r": 162, "spine": 52, "chest": 46,
    "neck": 30, "head": 38, "jaw": 14,
}
# how far each joint swings when its limb is flung out (+) or slams under (-)
react = {
    "shoulder_l": 52, "shoulder_r": 52, "elbow_l": 58, "elbow_r": 58,
    "wrist_l": 55, "wrist_r": 55, "hand_l": 30, "hand_r": 30,
    "thigh_l": 46, "thigh_r": 46, "knee_l": -46, "knee_r": -46,
    "ankle_l": 46, "ankle_r": 46, "spine": 7, "chest": 7,
    "neck": 9, "head": 16, "jaw": 14,
}
lag = {  # degrees of spin the limb trails behind
    "shoulder_l": 8, "shoulder_r": 8, "elbow_l": 14, "elbow_r": 14,
    "wrist_l": 22, "wrist_r": 22, "hand_l": 26, "hand_r": 26,
    "thigh_l": 9, "thigh_r": 9, "knee_l": 15, "knee_r": 15,
    "ankle_l": 22, "ankle_r": 22, "spine": 4, "chest": 6,
    "neck": 8, "head": 12, "jaw": 16,
}

def drive(bone, deg):
    """-1 when the limb is under the ball (crumple), +1 when it is on top (flail)."""
    dy, dz = LIMB[bone]
    r = math.hypot(dy, dz) or 1.0
    th = math.radians(deg - lag[bone])
    up = (dy * math.cos(th) - dz * math.sin(th)) / r
    # sharp: it drops in fast and hangs out on the way over
    return math.copysign(abs(max(-1.0, min(1.0, up))) ** 0.55, up)

# joints can't fold past their own stops
limit = {
    "knee_l": (112, 176), "knee_r": (112, 176),
    "elbow_l": (-176, -34), "elbow_r": (-176, -34),
    "shoulder_l": (-142, -18), "shoulder_r": (-142, -18),
    "thigh_l": (-168, -84), "thigh_r": (-168, -84),
    "ankle_l": (-40, 82), "ankle_r": (-40, 82),
}

tracks = {}
for bone, x0 in base.items():
    keys = []
    for i in range(K + 1):
        t = i / K
        rev = t * REVS                       # which turn we're on
        deg = 360.0 * (t * REVS % 1.0)       # position within that turn
        # each turn throws a little differently, and the difference cycles
        # smoothly so the clip still loops
        breathe = 1.0 + 0.55 * math.sin(2 * math.pi * (rev / REVS) + hash(bone) % 7)
        d = drive(bone, deg) * breathe
        x = x0 + react[bone] * d
        if bone in limit:
            x = max(limit[bone][0], min(limit[bone][1], x))
        keys.append([round(t * DUR, 3),
                     {"x": round(x, 1),
                      "z": round(react[bone] * 0.45 * d, 1)}])
    tracks[bone] = {"rotate": keys}

# contact: spin about the tucked centre, lowest limb pinned to y = 0
U = [((0.0, -0.051, 0.404), 0.10), ((0.149 - 0.0, 0.030 - CENT[1], -0.531 - CENT[2]), 0.05),
     ((-0.151, -0.020 - CENT[1], -0.061 - CENT[2]), 0.05),
     ((0.389, 0.264 - CENT[1], 0.105 - CENT[2]), 0.02),
     ((-0.378, 0.377 - CENT[1], -0.213 - CENT[2]), 0.02),
     ((0.016, 0.520 - CENT[1], 0.218 - CENT[2]), 0.19),
     ((0.088, 1.053 - CENT[1], 0.169 - CENT[2]), 0.06),
     ((-0.084, 1.162 - CENT[1], 0.065 - CENT[2]), 0.06)]
v = (0.0, CENT[1] - HIP0[1], CENT[2] - HIP0[2])
trans = []
for i in range(K + 1):
    th = math.radians(360.0 * (i / K * REVS % 1.0))
    h = -min(u[1] * math.cos(th) - u[2] * math.sin(th) - rr for u, rr in U)
    ry = v[1] * math.cos(th) - v[2] * math.sin(th)
    rz = v[1] * math.sin(th) + v[2] * math.cos(th)
    trans.append([round(i / K * DUR, 3), [0.0, round(h - H[1] - ry, 3), round(0.0 - H[2] - rz, 3)]])
tracks["hip"] = {"translate": trans,
                 "rotate": [[round(i / (12 * REVS) * DUR, 3), {"x": i * 30}]
                            for i in range(12 * REVS + 1)]}
op = {"type": "clip", "name": "roll", "skeleton": "hunter_rig", "duration": DUR,
      "loop": True, "interp": "linear", "tracks": tracks}
json.dump([op], open("crates/game/.modeler/gen/roll13.json", "w"), indent=1)
for b in ("elbow_l", "knee_l", "shoulder_r"):
    print(b, [k[1]["x"] for k in tracks[b]["rotate"]][:12])
