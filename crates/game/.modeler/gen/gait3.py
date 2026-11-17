import json, math

S = 1.88          # hip-to-ankle length in the rest pose
KEYS = 24

def leg(t, H, L, stance):
    """One leg over a cycle. Returns (hip, knee, ankle) in degrees about +x.
    hip>0 swings the leg back; knee>0 folds the shin back; ankle>0 points toes down."""
    # stance: foot planted, sweeps from -L (front) to +L (back)
    if t < stance:
        s = t / stance
        hip = -L + 2 * L * s
        # keep the foot on the floor: knee bend needed to reach it
        reach = min(1.0, H / S)
        knee = math.degrees(2 * math.asin(max(0.0, min(1.0, 1 - reach))) ) if False else 0.0
        # exact 2-bone solve for ankle under the hip at height H
        c = max(-1.0, min(1.0, H / S))
        knee = math.degrees(2 * math.acos(c))
        ankle = -hip * 0.85
        return hip, knee, ankle
    u = (t - stance) / (1.0 - stance)
    p = 0.5 - 0.5 * math.cos(math.pi * u)          # ease 0..1
    hip = L + (-2 * L) * p                          # back to front
    c = max(-1.0, min(1.0, (H + 0.10 * math.sin(math.pi * u)) / S))
    knee = math.degrees(2 * math.acos(c)) + 26 * math.sin(math.pi * u)
    ankle = -hip * 0.85 + 10 * math.sin(math.pi * u)
    return hip, knee, ankle

def clip(name, dur, H, L, stance, spine, chest, neck, head, swing, bob):
    tracks = {}
    for side, off in (("l", 0.0), ("r", 0.5)):
        for bone, idx in (("thigh", 0), ("knee", 1), ("ankle", 2)):
            pts = []
            for i in range(KEYS + 1):
                t = ((i / KEYS) + off) % 1.0
                pts.append([round(i / KEYS * dur, 3), {"x": round(leg(t, H, L, stance)[idx], 1)}])
            tracks[f"{bone}_{side}"] = {"rotate": pts}
        arm = []
        for i in range(KEYS + 1):
            t = ((i / KEYS) + off) % 1.0
            h = leg(t, H, L, stance)[0]
            arm.append([round(i / KEYS * dur, 3), {"x": round(-h * swing, 1)}])
        tracks[f"elbow_{side}"] = {"rotate": arm}
    tracks["hip"] = {"translate": [[round(i / KEYS * dur, 3),
        [0, round(-0.012 - bob * (0.5 + 0.5 * math.cos(4 * math.pi * i / KEYS)), 3), 0]]
        for i in range(KEYS + 1)]}
    for bone, val in (("spine", spine), ("chest", chest), ("neck", neck), ("head", head)):
        tracks[bone] = {"rotate": [[round(i / KEYS * dur, 3), {"x": val}] for i in range(KEYS + 1)]}
    tracks["jaw"] = {"rotate": [[round(i / KEYS * dur, 3), {"x": 6 if i % 2 else 1}] for i in range(KEYS + 1)]}
    return {"type": "clip", "name": name, "skeleton": "hunter_rig", "duration": dur,
            "loop": True, "interp": "smooth", "tracks": tracks}

ops = [
    # walk: gentle hunch, head level enough to stare ahead
    clip("walk", 2.4, 1.72, 16, 0.58, spine=10, chest=12, neck=8, head=-6, swing=0.7, bob=0.02),
    clip("run", 0.9, 1.66, 24, 0.44, spine=15, chest=17, neck=10, head=-10, swing=1.0, bob=0.04),
]
json.dump(ops, open("crates/game/.modeler/gen/gait3.json", "w"), indent=1)
for op in ops:
    print(op["name"])
    for b in ("thigh_l", "knee_l", "ankle_l", "thigh_r", "elbow_l"):
        print("  ", b, [k[1]["x"] for k in op["tracks"][b]["rotate"]][:10])
