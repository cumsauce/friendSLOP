import json, math

REVS, DUR = 3, 1.8
K = 36 * REVS
H = (0.0, 1.998, -0.009)
HIP0 = (0.0, 0.558, -0.529)
CENT = (0.0, 0.609, -0.126)
V = (0.0, CENT[1] - HIP0[1], CENT[2] - HIP0[2])
R = 0.82                       # the ball's radius: fixed, so it never bobs

def rot(y, z, deg):
    t = math.radians(deg)
    return y * math.cos(t) - z * math.sin(t), y * math.sin(t) + z * math.cos(t)

def smooth(t):
    t = max(0.0, min(1.0, t))
    return t * t * (3.0 - 2.0 * t)

# tucked: what a limb does when it's crushed under the body
base = {
    "thigh_l": -128, "thigh_r": -128, "knee_l": 162, "knee_r": 162,
    "ankle_l": 44, "ankle_r": 44, "shoulder_l": -88, "shoulder_r": -88,
    "elbow_l": -130, "elbow_r": -130, "wrist_l": -10, "wrist_r": -10,
    "hand_l": 160, "hand_r": 160, "spine": 46, "chest": 40,
    "neck": 28, "head": 34, "jaw": 12,
}
# reach: how far it flings out at the top (joints may bend unnaturally)
reach = {
    "thigh_l": -70, "thigh_r": -70, "knee_l": -55, "knee_r": -55,
    "ankle_l": 55, "ankle_r": 55, "shoulder_l": 85, "shoulder_r": 85,
    "elbow_l": 80, "elbow_r": 80, "wrist_l": 40, "wrist_r": 40,
    "hand_l": 25, "hand_r": 25, "spine": 5, "chest": 5,
    "neck": 6, "head": 10, "jaw": 12,
}
# crushed: extra fold while it's underneath, so nothing pokes out the bottom
crush = {
    "thigh_l": 14, "thigh_r": 14, "knee_l": 22, "knee_r": 22,
    "ankle_l": -20, "ankle_r": -20, "shoulder_l": -16, "shoulder_r": -16,
    "elbow_l": -26, "elbow_r": -26, "wrist_l": -18, "wrist_r": -18,
}
lag = {
    "thigh_l": 16, "thigh_r": 16, "knee_l": 28, "knee_r": 28,
    "ankle_l": 44, "ankle_r": 44, "shoulder_l": 22, "shoulder_r": 22,
    "elbow_l": 34, "elbow_r": 34, "wrist_l": 52, "wrist_r": 52,
    "hand_l": 60, "hand_r": 60, "spine": 8, "chest": 12,
    "neck": 16, "head": 24, "jaw": 28,
}

def extension(a):
    """a = where the limb is in the turn (0 = lowest). 0 crushed, 1 flung out.
    It stays crushed while it's under the body, reaches out as it climbs,
    hangs out over the top, then slams shut as it comes back down."""
    if a < 130.0:                       # underneath: crushed
        return 0.0
    if a < 265.0:                       # climbing: flings out
        return smooth((a - 130.0) / 135.0)
    if a < 300.0:                       # over the top, hanging out
        return 1.0
    return 1.0 - smooth((a - 300.0) / 60.0)   # slams shut before it lands

tracks = {}
for bone, x0 in base.items():
    keys = []
    for i in range(K + 1):
        t = i / K
        deg = 360.0 * (t * REVS % 1.0)
        e = extension((deg - lag[bone]) % 360.0)
        x = x0 + reach[bone] * e + crush.get(bone, 0.0) * (1.0 - e)
        keys.append([round(t * DUR, 3), {"x": round(x, 1),
                                         "z": round(reach[bone] * 0.25 * e, 1)}])
    tracks[bone] = {"rotate": keys}

trans = []
for i in range(K + 1):
    deg = 360.0 * (i / K * REVS % 1.0)
    ry, rz = rot(V[1], V[2], deg)
    trans.append([round(i / K * DUR, 3), [0.0, round(R - ry - H[1], 3), round(-rz - H[2], 3)]])
tracks["hip"] = {"translate": trans,
                 "rotate": [[round(i / (12 * REVS) * DUR, 3), {"x": i * 30}]
                            for i in range(12 * REVS + 1)]}
op = {"type": "clip", "name": "roll", "skeleton": "hunter_rig", "duration": DUR,
      "loop": True, "interp": "smooth", "tracks": tracks}
json.dump([op], open("crates/game/.modeler/gen/roll29.json", "w"), indent=1)

rates = []
for b, v in tracks.items():
    if b == "hip":
        continue
    ks = v["rotate"]
    rates.append((max(abs(ks[k+1][1]["x"] - ks[k][1]["x"]) for k in range(len(ks)-1)) / (DUR / K), b))
print("peak limb rate:", round(max(rates)[0]), "deg/s on", max(rates)[1])
print("vertical travel of the body:", 0.0, "m (fixed radius)")
