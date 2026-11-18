import json, math

A, B = 0.885, 0.935
ANKLE_Y = 0.117
KEY = 32

def ik(zf, yf, hip_y):
    dz, dy = zf, hip_y - yf
    d = math.hypot(dz, dy)
    d = min(d, A + B - 1e-4); d = max(d, abs(A - B) + 1e-4)
    line = math.degrees(math.atan2(dz, dy))
    ca = (A * A + d * d - B * B) / (2 * A * d)
    a = math.degrees(math.acos(max(-1.0, min(1.0, ca))))
    ck = (A * A + B * B - d * d) / (2 * A * B)
    knee = 180.0 - math.degrees(math.acos(max(-1.0, min(1.0, ck))))
    return -(line + a), knee

def foot(t, L, stance, lift):
    if t < stance:
        s = t / stance
        return L - 2 * L * s, ANKLE_Y
    u = (t - stance) / (1.0 - stance)
    p = 0.5 - 0.5 * math.cos(math.pi * u)
    return -L + 2 * L * p, ANKLE_Y + lift * math.sin(math.pi * u)

def lag(t, delay):
    return t - delay / (2 * math.pi)

def clip(name, dur, L, stance, lift, hip_drop, spine, chest, neck, head, swing, bob, sway):
    hip_y = A + B + ANKLE_Y - hip_drop
    tracks = {}
    # rest tracks the roll leaves turned: a clip that doesn't key them would
    # keep the roll's pose when it fades in from it (hip, shoulders, hands)
    zero = lambda: [[0.0, {"x": 0}], [dur, {"x": 0}]]
    for bone in ("shoulder_l", "shoulder_r", "hand_l", "hand_r"):
        tracks[bone] = {"rotate": zero()}

    def keys(fn):
        return [[round(i / KEY * dur, 3), fn(i / KEY)] for i in range(KEY + 1)]

    # legs: solved against the floor, with a slack knee and a loose ankle
    for side, off in (("l", 0.0), ("r", 0.5)):
        th, kn, an = [], [], []
        for i in range(KEY + 1):
            t = ((i / KEY) + off) % 1.0
            z, y = foot(t, L, stance, lift)
            h, k = ik(z, y, hip_y)
            a = -(h + k) * 0.85
            if t >= stance:
                u = (t - stance) / (1.0 - stance)
                a += 14 * math.sin(math.pi * u) + 5 * math.sin(3 * math.pi * u)
            else:
                # the knee gives a little under the weight, then catches
                k += 4 * math.sin(math.pi * (t / stance)) ** 2
            ts = round(i / KEY * dur, 3)
            th.append([ts, {"x": round(h, 1), "z": round(3.5 * math.sin(2 * math.pi * lag(t, 0.9)), 1)}])
            kn.append([ts, {"x": round(k, 1)}])
            an.append([ts, {"x": round(a, 1)}])
        tracks[f"thigh_{side}"] = {"rotate": th}
        tracks[f"knee_{side}"] = {"rotate": kn}
        tracks[f"ankle_{side}"] = {"rotate": an}
        # arms hang and swing late, like they're being dragged
        el = []
        for i in range(KEY + 1):
            t = ((i / KEY) + off) % 1.0
            z, _ = foot(t, L, stance, lift)
            slow = lag(t, 1.5)
            swing_deg = z * 60 * swing
            x = swing_deg + 9 * math.sin(2 * math.pi * slow + 1.2)
            el.append([round(i / KEY * dur, 3),
                       {"x": round(x, 1), "z": round(6 * math.sin(2 * math.pi * lag(t, 2.2) + 0.7), 1)}])
        tracks[f"elbow_{side}"] = {"rotate": el}
        # wrists flop a beat behind the forearm
        wr = []
        for i in range(KEY + 1):
            t = ((i / KEY) + off) % 1.0
            wr.append([round(i / KEY * dur, 3),
                       {"x": round(16 * math.sin(2 * math.pi * lag(t, 2.8) + 0.4), 1),
                        "z": round(14 * math.sin(2 * math.pi * lag(t, 2.4) + 2.1), 1)}])
        tracks[f"wrist_{side}"] = {"rotate": wr}

    tracks["hip"] = {"rotate": zero(), "translate": keys(lambda t: [
        round(-(sway / 70.0) * math.sin(2 * math.pi * t), 3),
        round(-hip_drop - bob * (0.5 + 0.5 * math.cos(4 * math.pi * t))
              + 0.012 * math.sin(2 * math.pi * lag(t, 1.0)), 3),
        0])}

    # the spine whips: every bone down the chain peaks later than its parent
    chain = (("spine", spine, 1.00, 0.00, 0.0),
             ("chest", chest, -0.80, 0.10, 2.5),
             ("neck",  neck,   0.70, 0.22, 5.0))
    for bone, pitch0, gain, delay, pdelay in chain:
        def f(t, pitch0=pitch0, gain=gain, delay=delay, pdelay=pdelay):
            roll = sway * gain * math.sin(2 * math.pi * lag(t, delay))
            pitch = pitch0 + 3.0 * math.sin(4 * math.pi * lag(t, pdelay))
            return {"x": round(pitch, 1), "z": round(roll, 1)}
        tracks[bone] = {"rotate": keys(f)}

    # the head lolls: it rolls further than the neck and lands last
    def hd(t):
        roll = -sway * 1.15 * math.sin(2 * math.pi * lag(t, 0.40)) \
               + 4.0 * math.sin(4 * math.pi * lag(t, 0.30) + 1.0)
        pitch = head + 6.0 * math.sin(4 * math.pi * lag(t, 0.45))
        return {"x": round(pitch, 1), "z": round(roll, 1)}
    tracks["head"] = {"rotate": keys(hd)}

    # the jaw swings open and shut on its own, slower than the steps
    tracks["jaw"] = {"rotate": keys(lambda t: {"x": round(
        12 + 14 * math.sin(2 * math.pi * lag(t, 0.6) + 0.8), 1)})}
    return {"type": "clip", "name": name, "skeleton": "hunter_rig", "duration": dur,
            "loop": True, "interp": "smooth", "tracks": tracks}

# Ground speed (m/s) of a clip = 2 * L / (stance * dur): the planted foot slides
# back 2L in the stance share of the cycle. The game moves the hunter at
# walk_speed 3.0 / chase_speed 6.5 (data/side/fulton.ron); `RigClip::moving`
# trims what's left, from the speeds scripts/gait-speed.py measures.
# The hips sink (hip_drop) so the longer stride still reaches the floor.
ops = [
    clip("walk", 0.80, 0.72, 0.60, 0.11, 0.20, spine=17, chest=21, neck=12, head=-34,
         swing=0.55, bob=0.02, sway=9.0),
    clip("run", 0.60, 0.85, 0.44, 0.20, 0.25, spine=22, chest=26, neck=14, head=-40,
         swing=0.8, bob=0.035, sway=11.0),
]
json.dump(ops, open("crates/game/.modeler/gen/gait8.json", "w"), indent=1)
for op in ops:
    print(op["name"], "tracks:", sorted(op["tracks"]))
    print("  head z", [k[1]["z"] for k in op["tracks"]["head"]["rotate"]][:8])
    print("  spine z", [k[1]["z"] for k in op["tracks"]["spine"]["rotate"]][:8])
