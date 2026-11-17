import json, math

# One leg's cycle from a stride: hip height H, half stride L, leg length S.
# Frame t in [0,1): stance t<0.5 then swing. Positive hip angle = leg behind.
S = 1.88

def leg_params(H, L):
    thigh = math.sqrt(max(H**2 + L**2 - S**2, 0.0))
    alpha = math.asin(min(1.0, thigh / S))  # half the beta angle
    beta_b = math.asin(L / H) + alpha
    beta_a = beta_b - 2 * alpha
    K = math.acos(H**2 + L**2) - A  # placeholder replaced below
    return beta_a, beta_b

def angles(H, L):
    thigh = math.sqrt(max(H**2 + L**2 - S**2, 0.0))
    alpha = math.asin(min(1.0, thigh / S))
    beta_b = math.asin(L / H) + alpha
    beta_a = beta_b - 2 * alpha
    # knee flexion b=2a, hip knee angle a
    K = 2 * math.asin(min(1.0, thigh / S))
    return beta_a, beta_b, K

# Walks: signed angles in degrees, z forward.
def walk_leg(t, H, L, stance=0.58):
    beta_a, beta_b, K = angles(H, L)
    a = math.degrees(beta_a); b = math.degrees(beta_b); k = math.degrees(K)
    if t < stance:
        s = t / stance
        hip = a + (b - a) * s          # -a (forward) -> +b (behind)
        knee = k                       # bent knee to clear the ground
        ankle = -hip * 0.85            # foot stays near level under the body
        return hip, knee, ankle
    u = (t - stance) / (1 - stance)    # swing 0..1
    p = 0.5 * (1 - math.cos(math.pi * u))
    peak = k + 30
    hip = b + (-a - b) * p
    # knee: straight at lift, max bend mid-swing, straight for heel strike
    knee = k + (peak - k) * math.sin(math.pi * min(1.0, u * 1.05)) ** 1.2
    # ankle: push toes down early, then level for heel strike
    ankle = -hip * 0.85 - 12 * math.sin(math.pi * u)
    return hip, knee, ankle

def make_clip(name, dur, H, L, stance, keys, spine, chest, swing, bob, head, hipside=0.0):
    def rot(v):
        return {"x": round(v, 1), "z": 0}
    tracks = {}
    def put(bone, ch, pairs):
        tracks[bone] = {ch: [[round(t*dur, 3), val] for t, val in pairs]}
    for side, off in (("l", 0.0), ("r", 0.5)):
        hip_pts, knee_pts, ankle_pts = [], [], []
        for i in range(keys + 1):
            t = (i / keys) % 1.0
            h, k, a = walk_leg((t + off) % 1.0, H, L, stance)
            hip_pts.append((i / keys, rot(h)))
            knee_pts.append((i / keys, rot(k)))
            ankle_pts.append((i / keys, rot(a)))
        put(f"thigh_{side}", "rotate", hip_pts)
        put(f"knee_{side}", "rotate", knee_pts)
        put(f"ankle_{side}", "rotate", ankle_pts)
        # arms swing opposite the same-side leg
        arm = []
        for i in range(keys + 1):
            t = (i / keys) % 1.0
            h, _, _ = walk_leg((t + off) % 1.0, H, L, stance)
            arm.append((i / keys, rot(-h * swing)))
        put(f"shoulder_{side}", "rotate", arm)
    # hips bob twice a cycle, lowest at mid-stance
    put("hip", "translate", [(i / keys, [0, round(-bob * (0.5 + 0.5 * math.cos(4 * math.pi * i / keys)), 3), 0]) for i in range(keys + 1)])
    put("spine", "rotate", [(i / keys, rot(spine)) for i in range(keys + 1)])
    put("chest", "rotate", [(i / keys, rot(chest + (1.5 if i % 2 else 0))) for i in range(keys + 1)])
    put("head", "rotate", [(0, rot(head)), (0.5, rot(head + 4)), (1.0, rot(head))][:3:1] if False else [(i / keys, rot(head + (4 if (i / keys) > 0.25 and (i / keys) < 0.75 else 0))) for i in range(keys + 1)])
    put("jaw", "rotate", [(i / keys, rot(8 if i % 2 else 2)) for i in range(keys + 1)])
    return {"type": "clip", "name": name, "skeleton": "hunter_rig", "duration": dur,
            "loop": True, "interp": "smooth", "tracks": tracks}

ops = [
    make_clip("walk", 2.4, 1.95, 0.42, 0.58, 24, spine=22, chest=22, swing=0.75, bob=0.03, head=8),
    make_clip("run", 0.9, 1.86, 0.7, 0.45, 20, spine=30, chest=28, swing=1.0, bob=0.06, head=-6),
]
json.dump(ops, open("crates/game/.modeler/gen/gait_ops.json", "w"), indent=1)
for op in ops:
    print(op["name"], {k: [round(v, 1) for v in [pt[1].get("x", 0) for pt in (c["rotate"] if "rotate" in c else [])][:0]] for k, c in list(op["tracks"].items())[:0]})
    for bone in ("thigh_l", "knee_l", "ankle_l", "thigh_r", "shoulder_l"):
        print(" ", bone, [pt[1]["x"] for pt in op["tracks"][bone]["rotate"]])
    print("  hip y", [pt[1][1] for pt in op["tracks"]["hip"]["translate"]])
