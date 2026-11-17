import json, math

DUR, K = 0.6, 24
H = (0.0, 1.998, -0.009)          # hip head at rest
HIP0 = (0.0, 0.558, -0.529)       # hip head in the tuck at 0 deg (measured)

# tucked joints at 0 deg (measured), with a rough radius for each extremity
J = [((0,0.558,-0.529),0.10),((0,0.786,-0.538),0.10),((-0.004,0.921,-0.298),0.12),
     ((-0.001,0.836,0.069),0.08),((0.016,0.520,0.218),0.19),((-0.001,0.589,0.124),0.08),
     ((0.202,0.917,-0.015),0.06),((0.231,0.567,-0.184),0.05),((0.357,0.726,0.090),0.04),
     ((0.369,0.824,0.144),0.04),((0.103,0.498,-0.520),0.07),((0.088,1.053,0.169),0.06),
     ((0.142,0.410,-0.510),0.05),((-0.208,0.906,-0.015),0.06),((-0.221,0.543,-0.156),0.05),
     ((-0.352,0.658,0.137),0.04),((-0.367,0.707,0.236),0.04),((-0.103,0.498,-0.520),0.07),
     ((-0.084,1.162,0.065),0.06),((-0.144,0.292,-0.279),0.05),((0.149,0.030,-0.531),0.05),
     ((-0.151,-0.020,-0.061),0.05),((0.389,0.264,0.105),0.02),((-0.378,0.377,-0.213),0.02)]

cy = sum(p[1] for p,_ in J)/len(J); cz = sum(p[2] for p,_ in J)/len(J)
v = (0.0, cy - HIP0[1], cz - HIP0[2])            # centroid, relative to the hip
U = [((p[0]-0.0, p[1]-HIP0[1]-v[1], p[2]-HIP0[2]-v[2]), r) for p, r in J]

def rot(y, z, th):
    return y*math.cos(th) - z*math.sin(th), y*math.sin(th) + z*math.cos(th)

base = {
    "thigh_l": -132, "thigh_r": -132, "knee_l": 162, "knee_r": 162,
    "ankle_l": 30, "ankle_r": 30, "shoulder_l": -84, "shoulder_r": -84,
    "elbow_l": -126, "elbow_r": -126, "wrist_l": -14, "wrist_r": -14,
    "hand_l": 162, "hand_r": 162, "spine": 52, "chest": 46,
    "neck": 30, "head": 38, "jaw": 14,
}
flap = {
    "thigh_l": (5, 1, 0.15), "thigh_r": (5, 1, 0.65), "knee_l": (8, 1, 0.30),
    "knee_r": (8, 1, 0.80), "ankle_l": (11, 2, 0.40), "ankle_r": (11, 2, 0.90),
    "shoulder_l": (7, 1, 0.45), "shoulder_r": (7, 1, 0.95), "elbow_l": (10, 1, 0.60),
    "elbow_r": (10, 1, 0.10), "wrist_l": (14, 2, 0.70), "wrist_r": (14, 2, 0.20),
    "hand_l": (8, 2, 0.75), "hand_r": (8, 2, 0.25), "spine": (3, 1, 0.10),
    "chest": (3, 1, 0.25), "neck": (4, 2, 0.50), "head": (9, 1, 0.55), "jaw": (9, 2, 0.35),
}
tracks = {}
for bone, x0 in base.items():
    amp, rate, phase = flap[bone]
    keys = []
    for i in range(K + 1):
        t = i / K
        wob = math.sin(2*math.pi*(rate*t + phase))
        loll = math.sin(2*math.pi*(rate*t + phase + 0.25))
        keys.append([round(t*DUR,3), {"x": round(x0+amp*wob,1), "z": round(amp*0.6*loll,1)}])
    tracks[bone] = {"rotate": keys}

trans = []
for i in range(K + 1):
    th = math.radians(360.0*i/K)
    # height that puts the lowest limb exactly on y = 0
    h = -min(rot(u[1], u[2], th)[0] - r for u, r in U)
    ry, rz = rot(v[1], v[2], th)
    trans.append([round(i/K*DUR,3),
                  [0.0, round(h - H[1] - ry, 3), round(0.0 - H[2] - rz, 3)]])
tracks["hip"] = {"translate": trans,
                 "rotate": [[round(i/8*DUR,3), {"x": i*45}] for i in range(9)]}
op = {"type":"clip","name":"roll","skeleton":"hunter_rig","duration":DUR,
      "loop":True,"interp":"linear","tracks":tracks}
json.dump([op], open("crates/game/.modeler/gen/roll7.json","w"), indent=1)
hs = [-t[1][1] for t in trans]
print(f"centroid rel hip: y={v[1]:+.3f} z={v[2]:+.3f}")
print(f"height range: {min(hs):.3f} .. {max(hs):.3f}  (bob {max(hs)-min(hs):.3f})")
