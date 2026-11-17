import json, math

REVS, DUR = 3, 1.8
K = 36 * REVS
H = (0.0, 1.998, -0.009)
HIP0 = (0.0, 0.558, -0.529)
CENT = (0.0, 0.609, -0.126)
V = (0.0, CENT[1] - HIP0[1], CENT[2] - HIP0[2])

# the posed body at 0 deg, out to its extremities (from the tuck), with a
# radius: enough to keep the silhouette off the floor
PTS = [((0.0, 0.0), 0.12),
       ((0.149, 0.264 - 0.0), 0.06),      # feet
       ((-0.151, 0.377 - 0.0), 0.06),
       ((0.389, 0.105), 0.03),            # fingertips
       ((-0.378, -0.213), 0.03),
       ((0.016, 0.218), 0.20),            # head
       ((0.088, 0.169), 0.08), ((-0.084, 0.065), 0.08)]   # knees

def rot(y, z, deg):
    t = math.radians(deg)
    return y * math.cos(t) - z * math.sin(t), y * math.sin(t) + z * math.cos(t)

def rest_yz(p):
    """a point given in world (y, z) at 0 deg -> offset from the ball centre"""
    return p[0] - CENT[1], p[1] - CENT[2]

def hip_y(deg):
    """Ball centre height: just clears the floor at this angle."""
    lows = []
    for (y, z), r in PTS:
        dy, dz = rest_yz((y, z))
        lows.append(rot(dy, dz, deg)[0] - r)
    lows.append(rot(-V[1], -V[2], deg)[0] - 0.12)
    return -min(lows)

# a limp body: fixed tuck inside the joint limits, everything trailing the spin
base = {
    "thigh_l": -128, "thigh_r": -128, "knee_l": 150, "knee_r": 150,
    "ankle_l": 44, "ankle_r": 44, "shoulder_l": -88, "shoulder_r": -88,
    "elbow_l": -122, "elbow_r": -122, "wrist_l": -10, "wrist_r": -10,
    "hand_l": 160, "hand_r": 160, "spine": 46, "chest": 40,
    "neck": 28, "head": 34, "jaw": 12,
}
# how far each joint drifts as the ball turns, and how late it lags
drift = {
    "thigh_l": 9, "thigh_r": 9, "knee_l": 11, "knee_r": 11,
    "ankle_l": 18, "ankle_r": 18, "shoulder_l": 12, "shoulder_r": 12,
    "elbow_l": 16, "elbow_r": 16, "wrist_l": 24, "wrist_r": 24,
    "hand_l": 14, "hand_r": 14, "spine": 4, "chest": 4,
    "neck": 6, "head": 10, "jaw": 12,
}
lag = {
    "thigh_l": 20, "thigh_r": 20, "knee_l": 34, "knee_r": 34,
    "ankle_l": 55, "ankle_r": 55, "shoulder_l": 26, "shoulder_r": 26,
    "elbow_l": 40, "elbow_r": 40, "wrist_l": 62, "wrist_r": 62,
    "hand_l": 72, "hand_r": 72, "spine": 8, "chest": 12,
    "neck": 18, "head": 26, "jaw": 30,
}
# nothing folds past its stops
limit = {
    "knee_l": (110, 176), "knee_r": (110, 176),
    "elbow_l": (-176, -30), "elbow_r": (-176, -30),
    "shoulder_l": (-145, -20), "shoulder_r": (-145, -20),
    "thigh_l": (-170, -80), "thigh_r": (-170, -80),
    "ankle_l": (-40, 85), "ankle_r": (-40, 85),
}

tracks = {}
for bone, x0 in base.items():
    keys = []
    for i in range(K + 1):
        t = i / K
        deg = 360.0 * (t * REVS % 1.0)
        # the limb is dragged round, so it swings with the turn but late
        w = math.sin(math.radians(deg - lag[bone]))
        x = x0 + drift[bone] * w
        if bone in limit:
            x = max(limit[bone][0], min(limit[bone][1], x))
        keys.append([round(t * DUR, 3), {"x": round(x, 1),
                                         "z": round(drift[bone] * 0.4 * math.cos(math.radians(deg - lag[bone])), 1)}])
    tracks[bone] = {"rotate": keys}

trans = []
for i in range(K + 1):
    deg = 360.0 * (i / K * REVS % 1.0)
    h = hip_y(deg)
    ry, rz = rot(V[1], V[2], deg)
    trans.append([round(i / K * DUR, 3), [0.0, round(h - ry - H[1], 3), round(-rz - H[2], 3)]])
tracks["hip"] = {"translate": trans,
                 "rotate": [[round(i / (12 * REVS) * DUR, 3), {"x": i * 30}]
                            for i in range(12 * REVS + 1)]}
op = {"type": "clip", "name": "roll", "skeleton": "hunter_rig", "duration": DUR,
      "loop": True, "interp": "smooth", "tracks": tracks}
json.dump([op], open("crates/game/.modeler/gen/roll24.json", "w"), indent=1)
dur = DUR
rates = []
for b, v in tracks.items():
    if b == "hip" or "rotate" not in v:
        continue
    ks = v["rotate"]
    rates.append((max(abs(ks[k+1][1]["x"] - ks[k][1]["x"]) for k in range(len(ks)-1)) / (dur / K), b))
worst = max(rates)
print("peak limb rate:", round(worst[0]), "deg/s on", worst[1])
