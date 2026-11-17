import json

DUR = 0.6
HINGE_Y = 2.0
RADIUS = 0.82

tuck = {
    "thigh_l": (-122, 0), "thigh_r": (-122, 0),   # knees up to the chest
    "knee_l": (150, 0), "knee_r": (150, 0),       # heels folded to the seat
    "ankle_l": (26, 0), "ankle_r": (26, 0),       # toes curled under
    "shoulder_l": (-74, 0), "shoulder_r": (-74, 0),
    "elbow_l": (-112, 0), "elbow_r": (-112, 0),
    "wrist_l": (-26, 0), "wrist_r": (-26, 0),
    "spine": (44, 0), "chest": (40, 0), "neck": (26, 0), "head": (34, 0),
    "jaw": (14, 0),
}

K = 16
tracks = {}
for bone, (x, z) in tuck.items():
    tracks[bone] = {"rotate": [[round(i / K * DUR, 3), {"x": x, "z": z}] for i in range(K + 1)]}
tracks["hip"] = {
    "translate": [[round(i / K * DUR, 3), [0, round(RADIUS - HINGE_Y, 3), 0]] for i in range(K + 1)],
    "rotate": [[round(i / 8 * DUR, 3), {"x": i * 45}] for i in range(9)],
}
op = {"type": "clip", "name": "roll", "skeleton": "hunter_rig", "duration": DUR,
      "loop": True, "interp": "linear", "tracks": tracks}
json.dump([op], open("crates/game/.modeler/gen/roll2.json", "w"), indent=1)
