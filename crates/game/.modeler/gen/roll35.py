# Grounded ragdoll tumble. Same flail as roll34, but on flat ground: the torso
# rides the floor (it never hangs in the air) and any limb that would go through
# it is folded / pushed back up (compressed), then springs out again.
import json, math, sys
sys.path.insert(0, __import__('os').path.dirname(__file__))
from hunter_fk import pose, euler, qmul, rot, sub, add, R, P
REVS, DUR = 4, 3.2
K = 64 * REVS
# slow variation: one cycle over the whole clip, so each of the 4 turns differs
SEED = {}
def var(name, k=1):
    if name not in SEED:
        SEED[name] = (len(SEED) * 2.39996) % (2 * math.pi)
    return lambda u: math.sin(k * u + SEED[name])
def sm(t): t = max(0.0, min(1.0, t)); return t*t*(3-2*t)
S = {"_l": 1, "_r": -1}
limbs = {
 "shoulder": (-55, 70, 48, 30), "elbow": (8, 12, 40, 70),
 "wrist": (0, 6, 18, 110), "hand": (0, 4, 14, 140),
 "thigh": (-35, 38, 40, 20), "knee": (4, 0, 6, 60), "ankle": (10, 0, 25, 100),
}
spine = {"spine": (14, 18, 10), "chest": (10, 14, 25), "neck": (-6, 24, 45),
         "head": (-8, 16, 55), "jaw": (20, 15, 90)}
TORSO_R = {"hip": 0.17, "spine": 0.15, "chest": 0.16, "neck": 0.1, "head": 0.24, "jaw": 0.12}
LIMB_R = 0.05
def limb_r(b):
    return 0.13 if b.startswith("hand") else 0.09 if b.startswith(("knee","ankle","elbow","wrist")) else 0.05
CHAINS = {f"{a}{s}": (f"{a}{s}", f"{b}{s}", f"{c}{s}") for s in S for a, b, c in
          (("shoulder", "elbow", "wrist"), ("thigh", "knee", "ankle"))}
ENDS = {"shoulder": ["elbow", "wrist", "hand", "claw"], "thigh": ["knee", "ankle", "ball", "toe"]}

def base(i):
    t = i / K; deg = 360 * t * REVS
    u = 2 * math.pi * t   # one cycle per clip
    loc = {}
    for b, (x0, sz, fx, lag) in limbs.items():
        for side, s in S.items():
            g = 1 + 0.18 * var(b + side + "a")(u) + 0.08 * var(b + side + "b", 3)(u)
            ph = math.radians(deg - lag + (0 if s > 0 else 137) + 28 * var(b + side + "p", 2)(u))
            loc[b + side] = {"x": x0 + g*(fx*math.sin(ph) + 0.3*fx*math.sin(2*ph + 1.3*s)),
                             "y": 0.0,
                             "z": s*g*(sz + 0.25*sz*math.sin(ph*2 + 0.7))}
    for b, (x0, amp, lag) in spine.items():
        ph = math.radians(deg - lag)
        loc[b] = {"x": x0 + amp*math.sin(ph), "y": 0.4*amp*math.cos(ph), "z": 0.5*amp*math.sin(2*ph + 0.4)}
    # the body turns in lurches: it slows as it lands on a side, then tips over fast
    # each turn a touch faster or slower, leaning and slewing a little differently
    pitch = deg + 4*math.sin(math.radians(deg*2)) + 9*math.sin(u + 0.7) + 4*math.sin(2*u + 2.1)
    loc["hip"] = {"x": pitch,
                  "y": 9*math.sin(2*u + 0.3) + 4*math.sin(3*u + 1.9),
                  "z": 10*math.sin(math.radians(deg)) + 8*math.sin(3*u + 1.0) + 4*math.sin(u + 2.5)}
    # sitting up (torso near upright): straight legs pike under and out in front
    for side, s_ in S.items():
        # starts folding well before upright and eases in, each leg a beat apart
        lagd = 0 if s_ > 0 else 18
        # a: how far past upright the torso is (0 = sitting, + = tipping forward)
        a_ = ((pitch - lagd) % 360 + 180) % 360 - 180
        c = sm((a_ + 130) / 80) * (1 - sm((a_ - 105) / 45))
        # straight out in front while sitting, folding tight to the chest as it
        # tips forward, so the legs go up and over instead of under
        tgt = -88 + 8*s_ - 72*sm(a_ / 95)
        loc["thigh"+side]["x"] = loc["thigh"+side]["x"]*(1-c) + tgt*c
        loc["thigh"+side]["z"] = loc["thigh"+side]["z"]*(1-0.85*c)
        loc["knee"+side]["x"] = loc["knee"+side]["x"]*(1-c) + 4*c
        loc["ankle"+side]["x"] = loc["ankle"+side]["x"]*(1-c) + 15*c
    return loc

def lowest(o, bones, r):
    return min(min(o[b][0][1], o[b][1][1]) - r(b) for b in bones)

def ground_torso(loc):
    o = pose(loc, [0, 0, 0])
    m = lowest(o, TORSO_R, lambda b: TORSO_R[b])
    return -m  # hip y offset so the torso just touches

def limb_bones(root, side):
    kind = root
    return [root + side] + [e + side for e in ENDS[kind]]

def fix_limb(loc, ty, root, side, prev=(0.0, 0.0, 0.0), wide=False):
    """Fold the limb until it is out of the floor; return the extra rotations."""
    mid = {"shoulder": "elbow", "thigh": "knee"}[root]
    bones = limb_bones(root, side)
    best = None
    o0 = pose(loc, [0, ty, 0])
    clear = lowest(o0, bones, limb_r) >= 0.0
    folds = range(0, 121, 10) if root == "thigh" else range(0, 121, 15)
    span = 170 if wide else 120
    dxs = range(-span, span + 1, 10) if root == "thigh" else range(-75, 76, 15)
    dzs = range(-90, 91, 15) if wide else range(-60, 61, 15)
    for fold in folds:
        for dx in dxs:
            for dz in dzs:
                # stay close to last frame's fix (no flipping), relax toward none
                cost = (4*(abs(dx - prev[0]) + abs(dz - prev[1])) + abs(fold - prev[2])
                        + 0.25*(abs(dx) + abs(dz)) + (0.05 if root == "thigh" else 0.15)*fold)
                if best and cost >= best[0]:
                    continue
                if root == "thigh" and not (-215 <= loc[root+side]["x"] + dx <= 75):
                    continue   # a hip only bends so far
                l2 = dict(loc)
                r = loc[root+side]; l2[root+side] = {"x": r["x"]+dx, "y": r["y"], "z": r["z"]+dz}
                m = loc[mid+side]
                sgn = -1   # knees bend backwards (hyperextend), elbows forward
                l2[mid+side] = {"x": m["x"]+sgn*fold, "y": m["y"], "z": m["z"]}
                if lowest(pose(l2, [0, ty, 0]), bones, limb_r) >= 0.0:
                    best = (cost, dx, dz, fold)
    return best[1:] if best else None

frames = []
PREV = {}
for i in range(K):
    loc = base(i)
    ty = ground_torso(loc)
    # if a limb can't fold out of the floor at this height, the body rides up
    # onto it (sitting up onto crumpled legs) instead of jamming it
    while True:
        fixes = {r+s_: fix_limb(loc, ty, r, s_, PREV.get(r+s_, (0.0, 0.0, 0.0))) for r in ("shoulder", "thigh") for s_ in S}
        for k, f in list(fixes.items()):
            if f is None:
                fixes[k] = fix_limb(loc, ty, k[:-2], k[-2:], PREV.get(k, (0.0, 0.0, 0.0)), wide=True)
        if all(f is not None for f in fixes.values()) or ty > ground_torso(loc) + 0.25:
            break
        ty += 0.04
    fixes = {k: (v if v is not None else (0.0, 0.0, 0.0)) for k, v in fixes.items()}
    PREV.update(fixes)
    frames.append((loc, ty, fixes))

# smooth the fixes through time (circular), keeping the max so nothing sinks:
# limbs fold a beat early and unfold a beat late, which reads as a squash
def spread(vals, w=1):
    n = len(vals); out = []
    for i in range(n):
        win = [vals[(i+k) % n] for k in range(-w, w+1)]
        out.append(sum(win) / len(win))
    return out
keys = list(frames[0][2].keys())
sm_fix = {}
for k in keys:
    w = 1
    sm_fix[k] = [spread([f[2][k][j] for f in frames], w) for j in range(3)]

n = len(frames)
need = [f[1] for f in frames]
eased = []
for i in range(n):
    w = [need[(i+k) % n] for k in range(-3, 4)]
    eased.append(max(need[i], 0.5*max(w) + 0.5*sum(w)/len(w)))
frames = [(f[0], eased[i], f[2]) for i, f in enumerate(frames)]
tracks = {}
def put(b, i, v):
    tracks.setdefault(b, {"rotate": []})["rotate"].append([round(i/K*DUR, 3), {a: round(v[a], 1) for a in v}])
hip_t = []
for i, (loc, ty, _) in enumerate(frames):
    for k in keys:
        root, side = k[:-2], k[-2:]
        mid = {"shoulder": "elbow", "thigh": "knee"}[root]
        dx, dz, fold = (sm_fix[k][j][i] for j in range(3))
        loc[root+side] = {"x": loc[root+side]["x"]+dx, "y": loc[root+side]["y"], "z": loc[root+side]["z"]+dz}
        sgn = -1
        loc[mid+side] = dict(loc[mid+side]); loc[mid+side]["x"] += sgn*fold
        lo, hi = (-125, 12) if mid == "knee" else (-95, 20)
        loc[mid+side]["x"] = max(lo, min(hi, loc[mid+side]["x"]))
    # thud: on the frames a big limb compresses, the body sinks a little more
    squash = sum(abs(sm_fix[k][2][i]) for k in keys) / 600.0
    y = ty + 0.03 - 0.04*min(1.0, squash)
    o = pose(loc, [0, y, 0])
    low = min(min(v[0][1], v[1][1]) - limb_r(b) for b, v in o.items() if b not in TORSO_R)
    low = min(low, lowest(o, TORSO_R, lambda b: TORSO_R[b]))
    if low < -0.01: y += -0.01 - low   # whatever still digs in lifts the body
    hip_t.append([round(i/K*DUR, 3), [0.0, round(y, 3), 0.0]])
    for b, v in loc.items():
        if b.startswith("claw"): continue
        put(b, i, v)
# the body settles, it doesn't teleport down: limit how fast it can fall
for _ in range(3):
    for i in range(len(hip_t)):
        prev = hip_t[i-1][1][1]
        if hip_t[i][1][1] < prev - 0.06:
            hip_t[i][1][1] = round(prev - 0.06, 3)
# close the loop
for b in tracks:
    first = tracks[b]["rotate"][0][1]
    if b == "hip":
        first = dict(first); first["x"] = first["x"] + 360*REVS
    tracks[b]["rotate"].append([DUR, first])
hip_t.append([DUR, hip_t[0][1]])
tracks["hip"]["translate"] = hip_t
tracks["hip"]["rotate"] = {"keys": tracks["hip"]["rotate"], "interp": "linear"}
json.dump([{"type": "clip", "name": "roll", "skeleton": "hunter_rig", "duration": DUR,
            "loop": True, "interp": "smooth", "tracks": tracks}],
          open("/Users/doomy/Code/friendSLOP/crates/game/.modeler/gen/roll35.json", "w"))
# report
mins = []
for i, (loc, ty, _) in enumerate(frames):
    o = pose(loc, hip_t[i][1])
    loc_b = None
    mins.append(min(min(v[0][1], v[1][1]) for v in o.values()))
print("lowest point per frame: min %.3f max %.3f" % (min(mins), max(mins)))
print("hip height range: %.2f..%.2f m" % (min(h[1][1] for h in hip_t)+1.998, max(h[1][1] for h in hip_t)+1.998))
