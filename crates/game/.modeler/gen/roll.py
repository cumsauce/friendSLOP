import json, math

DUR = 0.6            # one revolution
HINGE_Y = 2.0        # hip bone height in rest
RADIUS = 0.62        # the ball's radius / centre height

# tuck: knees to chest, arms wrapped, back curled, chin down
tuck = {
    "thigh_l": (-108, 0), "thigh_r": (-108, 0),
    "knee_l": (128, 0), "knee_r": (128, 0),
    "ankle_l": (-20, 0), "ankle_r": (-20, 0),
    "shoulder_l": (-58, 0), "shoulder_r": (-58, 0),
    "elbow_l": (-96, 0), "elbow_r": (-96, 0),
    "wrist_l": (-30, 0), "wrist_r": (-30, 0),
    "spine": (34, 0), "chest": (30, 0), "neck": (18, 0), "head": (26, 0),
    "jaw": (10, 0),
}

K = 16
tracks = {}
for bone in tuck:
    x, z = tuck[bone]
    tracks[bone] = {"rotate": [[round(i / K * DUR, 3), {"x": x, "z": z}] for i in range(K + 1)]}

# the spin: keys every 45 deg so a full turn can't be read as no turn
tracks["hip"] = {
    "translate": [[round(i / K * DUR, 3), [0, round(RADIUS - HINGE_Y, 3), 0]] for i in range(K + 1)],
    "rotate": [[round(i / 8 * DUR, 3), {"x": i * 45}] for i in range(9)],
}
op = {"type": "clip", "name": "roll", "skeleton": "hunter_rig", "duration": DUR,
      "loop": True, "interp": "linear", "tracks": tracks}
json.dump([op], open("crates/game/.modeler/gen/roll.json", "w"), indent=1)
print("hip:", op["tracks"]["hip"]["rotate"])
