import json, math
# Ragdoll tumble: the body cartwheels head over heels; limbs are flung out
# by the spin (splayed wide, joints hyper-extended), whipping behind it,
# folding only briefly when they're crushed under the body. L/R out of step.
REVS, DUR = 2, 1.6
K = 32 * REVS
H = (0.0, 1.998, -0.009)
HIP0 = (0.0, 0.558, -0.529)
CENT = (0.0, 0.609, -0.126)
V = (0.0, CENT[1] - HIP0[1], CENT[2] - HIP0[2])
R = 0.62
def rot(y, z, deg):
    t = math.radians(deg); return y*math.cos(t)-z*math.sin(t), y*math.sin(t)+z*math.cos(t)
def sm(t):
    t = max(0.0, min(1.0, t)); return t*t*(3-2*t)
# side sign: left +1, right -1 (z rotation flings a limb outward)
S = {"_l": 1, "_r": -1}
# bone: (base_x, splay_z, flop_x amplitude, lag deg, crushed extra x)
limbs = {
 "shoulder": (-60, 95, 55, 30, -50), "elbow": (8, 20, 45, 70, -70),
 "wrist": (0, 15, 40, 110, -30), "hand": (0, 10, 35, 140, 0),
 "thigh": (-35, 55, 45, 20, 40), "knee": (12, 0, 50, 60, 60),
 "ankle": (10, 0, 35, 100, 0),
}
spine = {"spine": (10, 18, 10), "chest": (8, 14, 25), "neck": (-10, 30, 45),
         "head": (-15, 35, 70), "jaw": (20, 15, 90)}
def crushed(a):  # a: angle of the limb in the turn, 0 = underneath
    a %= 360
    d = min(a, 360 - a)
    return 1 - sm((d - 25) / 45)   # folds within ~70 deg of the bottom
tracks = {}
for b, (x0, sz, fx, lag, cx) in limbs.items():
    for side, s in S.items():
        off = 0 if s > 0 else 137   # the two sides out of step
        keys = []
        for i in range(K + 1):
            t = i / K; deg = 360 * (t * REVS)
            ph = math.radians(deg - lag + off)
            c = crushed(deg - lag + off * 0.3 + (0 if b in ("thigh","knee","ankle") else 180))
            x = x0 + fx * math.sin(ph) + 0.3 * fx * math.sin(2 * ph + 1.3 * s) + cx * c
            z = s * (sz * (1 - 0.6 * c) + 0.25 * sz * math.sin(ph * 2 + 0.7))
            y = s * 20 * math.sin(ph + 0.5) if b in ("shoulder", "thigh") else 0
            keys.append([round(t * DUR, 3), {"x": round(x, 1), "y": round(y, 1), "z": round(z, 1)}])
        tracks[b + side] = {"rotate": keys}
for b, (x0, amp, lag) in spine.items():
    keys = []
    for i in range(K + 1):
        t = i / K; ph = math.radians(360 * t * REVS - lag)
        keys.append([round(t*DUR, 3), {"x": round(x0 + amp*math.sin(ph), 1),
                     "z": round(0.5*amp*math.sin(ph*2+0.4), 1), "y": round(0.4*amp*math.cos(ph), 1)}])
    tracks[b] = {"rotate": keys}
trans, hr = [], []
for i in range(K + 1):
    t = i / K; deg = 360 * t * REVS
    ry, rz = rot(V[1], V[2], deg)
    bump = 0.08 * abs(math.sin(math.radians(deg * 2)))  # it bounces on its corners
    trans.append([round(t*DUR, 3), [round(0.06*math.sin(math.radians(deg*1.5)), 3),
                  round(R + bump - ry - H[1], 3), round(-rz - H[2], 3)]])
    hr.append([round(t*DUR, 3), {"x": round(deg, 1), "z": round(12*math.sin(math.radians(deg)), 1)}])
tracks["hip"] = {"translate": trans, "rotate": {"keys": hr, "interp": "linear"}}
json.dump([{"type": "clip", "name": "roll", "skeleton": "hunter_rig", "duration": DUR,
            "loop": True, "interp": "smooth", "tracks": tracks}],
          open("crates/game/.modeler/gen/roll34.json", "w"))
