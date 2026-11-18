#!/usr/bin/env python3
"""Measure how fast each looping clip of a rigged glTF moves over the ground.

    scripts/gait-speed.py crates/game/assets/models/features/fs_hunter.gltf [more.gltf ..] [--clips=walk,run] [--write]

Forward kinematics on the skeleton (plain python, no numpy): for every clip,
sample the pose, follow each foot-ish joint (ankle/ball/toe/foot/paw) in the
root's frame, and take the stretches where it is planted (near its lowest) and
sliding backwards. A planted foot moves back at exactly the speed the body
moves forward, so the mean of that is the clip's ground speed in m/s.
Nothing is printed for a clip with no feet, or that stands still.

--write merges the results into crates/game/assets/data/base/gaits.ron
(model path under assets/ -> clip -> m/s), which `RigClip::moving` reads.
"""
import json, math, os, re, struct, sys

FEET = ("ankle", "ball", "toe", "foot", "paw", "heel")
HERE = os.path.dirname(os.path.abspath(__file__))
RON = os.path.join(HERE, "..", "crates", "game", "assets", "data", "base", "gaits.ron")


def qmul(a, b):
    ax, ay, az, aw = a; bx, by, bz, bw = b
    return (aw*bx+ax*bw+ay*bz-az*by, aw*by-ax*bz+ay*bw+az*bx,
            aw*bz+ax*by-ay*bx+az*bw, aw*bw-ax*bx-ay*by-az*bz)


def rot(q, v):
    x, y, z, w = q
    p = qmul(qmul(q, (v[0], v[1], v[2], 0)), (-x, -y, -z, w))
    return p[:3]


def slerp(a, b, t):
    d = sum(x*y for x, y in zip(a, b))
    if d < 0:
        b = [-x for x in b]; d = -d
    if d > 0.9995:
        q = [x + t*(y-x) for x, y in zip(a, b)]
    else:
        th = math.acos(d); s = math.sin(th)
        q = [(math.sin((1-t)*th)*x + math.sin(t*th)*y)/s for x, y in zip(a, b)]
    n = math.sqrt(sum(x*x for x in q))
    return tuple(x/n for x in q)


class Gltf:
    def __init__(self, path):
        self.dir = os.path.dirname(path)
        self.g = json.load(open(path))
        self.bufs = [open(os.path.join(self.dir, b["uri"]), "rb").read() for b in self.g["buffers"]]

    def accessor(self, i):
        a = self.g["accessors"][i]
        v = self.g["bufferViews"][a["bufferView"]]
        n = {"SCALAR": 1, "VEC3": 3, "VEC4": 4}[a["type"]]
        off = v.get("byteOffset", 0) + a.get("byteOffset", 0)
        data = self.bufs[v["buffer"]]
        stride = v.get("byteStride") or n * 4
        out = []
        for k in range(a["count"]):
            out.append(struct.unpack_from("<%df" % n, data, off + k * stride))
        return out


def sample(times, vals, t, quat):
    if t <= times[0]:
        return vals[0]
    if t >= times[-1]:
        return vals[-1]
    lo, hi = 0, len(times) - 1
    while hi - lo > 1:
        mid = (lo + hi) // 2
        if times[mid] <= t: lo = mid
        else: hi = mid
    f = (t - times[lo]) / (times[hi] - times[lo])
    a, b = vals[lo], vals[hi]
    if quat:
        return slerp(a, b, f)
    return tuple(x + f*(y-x) for x, y in zip(a, b))


def measure(path, samples=240):
    gl = Gltf(path)
    g = gl.g
    nodes = g["nodes"]
    parent = {}
    for i, n in enumerate(nodes):
        for c in n.get("children", []):
            parent[c] = i
    joints = g["skins"][0]["joints"] if g.get("skins") else []
    feet = [j for j in joints if any(f in nodes[j]["name"].lower() for f in FEET)]
    if not feet:
        return {}
    # the root is the joint whose parent isn't a joint
    jset = set(joints)
    roots = [j for j in joints if parent.get(j) not in jset]

    def chain(j):
        c = []
        while j in jset:
            c.append(j); j = parent[j]
        return c[::-1]

    res = {}
    for anim in g["animations"]:
        tracks = {}
        dur = 0.0
        for ch in anim["channels"]:
            s = anim["samplers"][ch["sampler"]]
            t = [x[0] for x in gl.accessor(s["input"])]; v = gl.accessor(s["output"])
            dur = max(dur, t[-1])
            tracks[(ch["target"]["node"], ch["target"]["path"])] = (t, v)
        if dur <= 0:
            continue

        def foot_pos(j, time):
            p = (0.0, 0.0, 0.0); q = (0, 0, 0, 1)
            # world transform = fold from the root joint down (root frame, parent ignored)
            pos = None
            for n in chain(j):
                nd = nodes[n]
                tr = tracks.get((n, "translation"))
                lt = sample(*tr, time, False) if tr else nd.get("translation", (0, 0, 0))
                rt = tracks.get((n, "rotation"))
                lq = sample(*rt, time, True) if rt else nd.get("rotation", (0, 0, 0, 1))
                if pos is None:
                    pos = tuple(lt); q = tuple(lq)
                else:
                    d = rot(q, lt)
                    pos = tuple(pos[i] + d[i] for i in range(3)); q = qmul(q, tuple(lq))
            return pos

        speeds = []
        for f in feet:
            # forward is +z (the models face +z)
            pts = [foot_pos(f, dur * k / samples) for k in range(samples)]
            ys = [p[1] for p in pts]
            lo, hi = min(ys), max(ys)
            if hi - lo < 1e-4:
                continue
            plant = lo + 0.2 * (hi - lo)
            dt = dur / samples
            vs = []
            for k in range(samples):
                a, b = pts[k], pts[(k + 1) % samples]
                if a[1] <= plant and b[1] <= plant:
                    vs.append(-(b[2] - a[2]) / dt)
            moving = [v for v in vs if v > 0.05]
            if len(moving) >= 4:
                speeds.append(sum(moving) / len(moving))
        if speeds:
            res[anim["name"]] = sum(speeds) / len(speeds)
    return res


def asset_key(path):
    p = os.path.abspath(path)
    i = p.find("/assets/")
    return p[i + len("/assets/"):] if i >= 0 else os.path.basename(p)


def read_ron():
    out = {}
    if not os.path.exists(RON):
        return out
    cur = None
    for line in open(RON):
        m = re.match(r'\s*"([^"]+)":\s*\{', line)
        if m:
            cur = out.setdefault(m.group(1), {})
            continue
        m = re.match(r'\s*"([^"]+)":\s*([0-9.]+),', line)
        if m and cur is not None:
            cur[m.group(1)] = float(m.group(2))
    return out


HEADER = """// Ground speed of each looping rigged clip (m/s): how fast a walker the clip
// is drawn on is really moving when its feet stay planted. A rig playing
// `RigClip::moving(speed)` (crates/base/src/rigged.rs) plays the clip at
// speed / this. Measured by scripts/gait-speed.py, never by hand: re-run
//   scripts/gait-speed.py <model.gltf> --write
// after re-authoring a clip. A clip not listed plays at its authored speed.
"""


def limits():
    lo, hi = "0.4", "2.5"
    if os.path.exists(RON):
        t = open(RON).read()
        m = re.search(r"min_playback:\s*([0-9.]+)", t); lo = m.group(1) if m else lo
        m = re.search(r"max_playback:\s*([0-9.]+)", t); hi = m.group(1) if m else hi
    return lo, hi


def write_ron(data):
    s = HEADER + "(\n    // Slowest and fastest playback a walker's pace may ask for (x authored).\n"
    s += "    min_playback: %s,\n    max_playback: %s,\n    models: {\n" % (limits()[0], limits()[1])
    for model in sorted(data):
        s += f'        "{model}": {{\n'
        for clip in sorted(data[model]):
            s += f'            "{clip}": {data[model][clip]:.2f},\n'
        s += "        },\n"
    s += "    },\n)\n"
    open(RON, "w").write(s)


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    only = next((a[8:].split(",") for a in sys.argv if a.startswith("--clips=")), None)
    if not args:
        sys.exit(__doc__)
    merged = read_ron() if "--write" in sys.argv else {}
    for path in args:
        r = {k: v for k, v in measure(path).items() if only is None or k in only}
        for clip, v in sorted(r.items()):
            print(f"{asset_key(path)}  {clip}: {v:.2f} m/s")
        if r:
            merged.setdefault(asset_key(path), {}).update({k: round(v, 2) for k, v in r.items()})
    if "--write" in sys.argv:
        write_ron(merged)


if __name__ == "__main__":
    main()
