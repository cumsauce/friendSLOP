import json, math

A, B = 0.885, 0.935      # thigh, shin (hip->knee, knee->ankle), metres
ANKLE_Y = 0.117          # ankle height with the foot flat on the floor
KEY = 24

def ik(zf, yf, hip_y):
    """Two-bone IK in the sagittal plane (forward = +z, up = +y).
    Hip at (0, hip_y), ankle at (zf, yf). Returns degrees:
    thigh (+ = swings back), knee (+ = shin folds back)."""
    dz, dy = zf, hip_y - yf
    d = math.hypot(dz, dy)
    d = min(d, A + B - 1e-4)
    d = max(d, abs(A - B) + 1e-4)
    # angle of the hip->ankle line, forward from straight down
    line = math.degrees(math.atan2(dz, dy))
    # angle between the thigh and that line
    cos_a = (A * A + d * d - B * B) / (2 * A * d)
    a = math.degrees(math.acos(max(-1.0, min(1.0, cos_a))))
    cos_k = (A * A + B * B - d * d) / (2 * A * B)
    knee = 180.0 - math.degrees(math.acos(max(-1.0, min(1.0, cos_k))))
    thigh_fwd = line + a          # thigh forward of vertical
    return -thigh_fwd, knee       # +x rotation swings the leg BACK

def foot(t, L, stance, lift):
    if t < stance:
        s = t / stance
        return L - 2 * L * s, ANKLE_Y          # planted, front -> back
    u = (t - stance) / (1.0 - stance)
    p = 0.5 - 0.5 * math.cos(math.pi * u)
    z = -L + 2 * L * p
    y = ANKLE_Y + lift * math.sin(math.pi * u)
    return z, y

def clip(name, dur, L, stance, lift, hip_drop, spine, chest, neck, head, swing, bob):
    hip_y = A + B + ANKLE_Y - hip_drop
    tracks = {}
    for side, off in (("l", 0.0), ("r", 0.5)):
        th, kn, an = [], [], []
        for i in range(KEY + 1):
            t = ((i / KEY) + off) % 1.0
            z, y = foot(t, L, stance, lift)
            h, k = ik(z, y, hip_y)
            # keep the sole flat: undo what the thigh and knee did
            a = -(h + k) * 0.9
            if t >= stance:                        # toes up on the way through
                u = (t - stance) / (1.0 - stance)
                a += 14 * math.sin(math.pi * u)
            ts = round(i / KEY * dur, 3)
            th.append([ts, {"x": round(h, 1)}])
            kn.append([ts, {"x": round(k, 1)}])
            an.append([ts, {"x": round(a, 1)}])
        tracks[f"thigh_{side}"] = {"rotate": th}
        tracks[f"knee_{side}"] = {"rotate": kn}
        tracks[f"ankle_{side}"] = {"rotate": an}
        arm = []
        for i in range(KEY + 1):
            t = ((i / KEY) + off) % 1.0
            z, _ = foot(t, L, stance, lift)
            arm.append([round(i / KEY * dur, 3), {"x": round(z * 60 * swing, 1)}])
        tracks[f"elbow_{side}"] = {"rotate": arm}
    tracks["hip"] = {"translate": [[round(i / KEY * dur, 3),
        [0, round(-hip_drop - bob * (0.5 + 0.5 * math.cos(4 * math.pi * i / KEY)), 3), 0]]
        for i in range(KEY + 1)]}
    for bone, val in (("spine", spine), ("chest", chest), ("neck", neck), ("head", head)):
        tracks[bone] = {"rotate": [[round(i / KEY * dur, 3), {"x": val}] for i in range(KEY + 1)]}
    tracks["jaw"] = {"rotate": [[round(i / KEY * dur, 3), {"x": 6 if i % 2 else 1}] for i in range(KEY + 1)]}
    return {"type": "clip", "name": name, "skeleton": "hunter_rig", "duration": dur,
            "loop": True, "interp": "smooth", "tracks": tracks}

ops = [
    clip("walk", 2.4, 0.30, 0.60, 0.10, 0.06, spine=8, chest=10, neck=6, head=-6, swing=0.55, bob=0.015),
    clip("run", 0.9, 0.55, 0.45, 0.20, 0.09, spine=12, chest=14, neck=8, head=-10, swing=0.8, bob=0.03),
]
json.dump(ops, open("crates/game/.modeler/gen/gait4.json", "w"), indent=1)
for op in ops:
    print(op["name"])
    for b in ("thigh_l", "knee_l", "ankle_l", "thigh_r", "elbow_l"):
        print("  ", b, [k[1]["x"] for k in op["tracks"][b]["rotate"]][:12])
