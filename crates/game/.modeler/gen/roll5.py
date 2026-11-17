import json, math

DUR, HINGE_Y, K = 0.6, 1.998, 24
BALL_R = 0.70                      # set from the measurement pass
# tucked mass centre relative to the hip, measured from the posed skeleton
CENTROID = (0.0, 0.142, 0.529)

base = {
    "thigh_l": -122, "thigh_r": -122, "knee_l": 150, "knee_r": 150,
    "ankle_l": 55, "ankle_r": 55, "shoulder_l": -74, "shoulder_r": -74,
    "elbow_l": -112, "elbow_r": -112, "wrist_l": -26, "wrist_r": -26,
    "hand_l": 155, "hand_r": 155, "spine": 44, "chest": 40,
    "neck": 26, "head": 34, "jaw": 14,
}
flap = {
    "thigh_l": (10, 1, 0.15), "thigh_r": (10, 1, 0.65),
    "knee_l": (14, 1, 0.30), "knee_r": (14, 1, 0.80),
    "ankle_l": (20, 2, 0.40), "ankle_r": (20, 2, 0.90),
    "shoulder_l": (12, 1, 0.45), "shoulder_r": (12, 1, 0.95),
    "elbow_l": (18, 1, 0.60), "elbow_r": (18, 1, 0.10),
    "wrist_l": (26, 2, 0.70), "wrist_r": (26, 2, 0.20),
    "hand_l": (14, 2, 0.75), "hand_r": (14, 2, 0.25),
    "spine": (5, 1, 0.10), "chest": (6, 1, 0.25),
    "neck": (8, 2, 0.50), "head": (16, 1, 0.55), "jaw": (16, 2, 0.35),
}

tracks = {}
for bone, x0 in base.items():
    amp, rate, phase = flap[bone]
    keys = []
    for i in range(K + 1):
        t = i / K
        wob = math.sin(2 * math.pi * (rate * t + phase))
        loll = math.sin(2 * math.pi * (rate * t + phase + 0.25))
        keys.append([round(t * DUR, 3),
                     {"x": round(x0 + amp * wob, 1), "z": round(amp * 0.6 * loll, 1)}])
    tracks[bone] = {"rotate": keys}

# spin about the tucked body's own centre, not the hip: undo the orbit the
# rotation would otherwise impose on that centre
trans = []
for i in range(K + 1):
    th = math.radians(360.0 * i / K)
    cy, cz = CENTROID[1], CENTROID[2]
    ry = cy * math.cos(th) - cz * math.sin(th)
    rz = cy * math.sin(th) + cz * math.cos(th)
    trans.append([round(i / K * DUR, 3),
                  [0.0, round(BALL_R - HINGE_Y - ry, 3), round(-ry * 0 - rz + 0.009, 3)]])
tracks["hip"] = {
    "translate": trans,
    "rotate": [[round(i / 8 * DUR, 3), {"x": i * 45}] for i in range(9)],
}
op = {"type": "clip", "name": "roll", "skeleton": "hunter_rig", "duration": DUR,
      "loop": True, "interp": "linear", "tracks": tracks}
json.dump([op], open("crates/game/.modeler/gen/roll5.json", "w"), indent=1)
print("hip translate sample:", [r[1] for r in trans[:4]])
