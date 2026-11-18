#!/usr/bin/env python3
"""The overgrown ruins' and the foundry's courses (surf stages 10-15).

Prints, per pack, the pads and the ramps' ridge points to paste into
crates/game/assets/data/side/surf_{overgrown,foundry}.ron (`--write` puts them
there and moves the scenery that stands at absolute heights down with them).

Every stage: off the pad a steep dive, a 10 m hop, then a split. The main line
swings out round the stage's landmark (one side, then the other, stage to
stage) and comes back; the shortcut runs straight past the landmark's other
side and jumps a speed gap. Both land side by side on the next pad. The ramps
fall `MAIN_SLOPE` m a metre along the main line (the shortcut, shorter, a
little steeper): quick, with no flat cruise and no loops.
"""
import math, re, sys

DIVE = (40.0, 0.22)  # the run off a pad: length (m), slope
HOP = 10.0  # the hop from the dive to the split
MAIN_HOP = 11.5  # the main line's one hop
SPLIT = 6.5  # each line this far either side of the middle at a split and a landing
MAIN_SLOPE = 0.15
GAP = (22.0, 6.0)  # the shortcut's speed gap: length (m), extra drop over it
STEP = 9.0  # metres between ridge points

# (stage, pad x, pad z, pad size, turn angle (deg), arc radius, out leg (m), side +1/-1)
# side: which way the main line swings (+1 = +z, south; -1 = north).
STAGES = [
    (10, 3200.0, -1935.0, 16.0, 45.0, 72.0, 12.0, -1),
    (11, 3640.0, -1960.0, 20.0, 45.0, 72.0, 4.0, 1),
    (12, 4030.0, -1945.0, 20.0, 45.0, 70.0, 22.0, -1),
    (13, 4400.0, -1935.0, 16.0, 45.0, 75.0, 16.0, 1),
    (14, 4790.0, -1935.0, 16.0, 55.0, 70.0, 24.0, -1),
    (15, 5190.0, -1935.0, 16.0, 50.0, 80.0, 16.0, 1),
]
FINISH = (5600.0, -1935.0)
TOP = -905.0  # P10's top (the neon's last landing: fixed)


def walk(start, heading, segs):
    """Points every <= STEP m along straights ('S', L) and arcs ('A', deg, R)."""
    x, z = start
    a = heading
    pts = [(x, z)]
    for s in segs:
        if s[0] == "S":
            n = max(1, math.ceil(s[1] / STEP))
            for i in range(n):
                x += math.cos(a) * s[1] / n
                z += math.sin(a) * s[1] / n
                pts.append((x, z))
        else:
            deg, r = s[1], s[2]
            turn = math.radians(deg)
            n = max(2, math.ceil(abs(turn) * r / STEP))
            sign = 1 if turn > 0 else -1
            cx, cz = x - sign * math.sin(a) * r, z + sign * math.cos(a) * r
            for i in range(n):
                a += turn / n
                x, z = cx + sign * math.sin(a) * r, cz - sign * math.cos(a) * r
                pts.append((x, z))
    return pts, a


def resample(pts, step):
    """The same line, points every `step` m (ends kept)."""
    total = sum(math.dist(pts[i], pts[i + 1]) for i in range(len(pts) - 1))
    n = max(2, round(total / step))
    out, i, s0 = [pts[0]], 0, 0.0
    for k in range(1, n):
        want = total * k / n
        while s0 + math.dist(pts[i], pts[i + 1]) < want:
            s0 += math.dist(pts[i], pts[i + 1])
            i += 1
        f = (want - s0) / math.dist(pts[i], pts[i + 1])
        out.append((pts[i][0] + (pts[i + 1][0] - pts[i][0]) * f, pts[i][1] + (pts[i + 1][1] - pts[i][1]) * f))
    out.append(pts[-1])
    return out


def length(pts):
    return sum(math.dist(pts[i], pts[i + 1]) for i in range(len(pts) - 1))


def detour(start, end, deg, r, out_leg, side):
    """East to east from `start` to `end`: out (turn, leg, turn back), a middle
    straight, back (turn, leg, turn back). Solves the middle and the out leg."""
    t = side * deg

    def arc(t):
        # Eased in and out (half the curvature at each end): the ridge's
        # spline overshoots a sudden change of curvature.
        return [("A", t * 0.15, 2 * r), ("A", t * 0.7, r), ("A", t * 0.15, 2 * r)]

    def segs(mid, leg):
        return [("S", 12.0), *arc(t), ("S", leg), *arc(-t), ("S", mid),
                *arc(-t), ("S", out_leg), *arc(t), ("S", 12.0)]

    p0, _ = walk(start, 0.0, segs(0.0, out_leg))
    dx, dz = end[0] - p0[-1][0], end[1] - p0[-1][1]
    th = math.radians(t)
    # leg extra e: (e cos th, e sin th); mid m: (m, 0)
    e = dz / math.sin(th)
    m = dx - e * math.cos(th)
    leg, mid = out_leg + e, m
    if leg < 0 or mid < 44.0:
        raise ValueError(f"detour from {start} to {end} doesn't fit: leg {leg:.1f} mid {mid:.1f}")
    pts, a = walk(start, 0.0, segs(mid, leg))
    pts = resample(pts, STEP)
    assert abs(pts[-1][0] - end[0]) < 0.05 and abs(pts[-1][1] - end[1]) < 0.05, (pts[-1], end)
    return pts


def heights(pts, y0, y1):
    total = length(pts)
    out, s = [], 0.0
    for i, (x, z) in enumerate(pts):
        if i:
            s += math.dist(pts[i - 1], pts[i])
        out.append((x, y0 + (y1 - y0) * s / total, z))
    return out


def cut(pts3, at, gap):
    """Split a line at arc length `at`, leaving out `gap` m."""
    s, a, b = 0.0, [pts3[0]], []
    for i in range(1, len(pts3)):
        s += math.dist(pts3[i - 1][::2], pts3[i][::2])
        if s <= at - gap / 2:
            a.append(pts3[i])
        elif s >= at + gap / 2:
            b.append(pts3[i])
    return a, b


def lerp_at(pts3, at):
    s = 0.0
    for i in range(1, len(pts3)):
        d = math.dist(pts3[i - 1][::2], pts3[i][::2])
        if s + d >= at:
            f = (at - s) / d
            return tuple(pts3[i - 1][k] + (pts3[i][k] - pts3[i - 1][k]) * f for k in range(3))
        s += d
    return pts3[-1]


def trim_to(pts3, s0, s1):
    """The part of a line from arc length s0 to s1, ends exact."""
    out, s = [lerp_at(pts3, s0)], 0.0
    for i in range(1, len(pts3)):
        s += math.dist(pts3[i - 1][::2], pts3[i][::2])
        if s0 + STEP / 2 < s < s1 - STEP / 2:
            out.append(pts3[i])
    out.append(lerp_at(pts3, s1))
    return out


def build():
    pads, ramps, gaps = [], [], []
    y = TOP
    for i, (st, px, pz, size, deg, r, leg, side) in enumerate(STAGES):
        nx, nz = (STAGES[i + 1][1], STAGES[i + 1][2]) if i + 1 < len(STAGES) else FINISH
        pads.append((st, px, y, pz, size))
        # The dive.
        a0 = (px + 12.35, y - 5.5, pz)
        a1 = (a0[0] + DIVE[0], a0[1] - DIVE[0] * DIVE[1], pz)
        dive = [a0, ((a0[0] + a1[0]) / 2, (a0[1] + a1[1]) / 2, pz), a1]
        fy = a1[1] - 3.0
        fx = a1[0] + HOP
        end_x = nx - 30.0
        # Main: out round the landmark.
        m_start, m_end = (fx, pz + side * SPLIT), (end_x, nz + side * SPLIT)
        # The sharpest swing (up to `deg`) that fits the stage.
        for tryd in range(int(deg), 25, -5):
            try:
                flat = detour(m_start, m_end, tryd, r, leg, side)
                break
            except ValueError:
                continue
        else:
            raise SystemExit(f"stage {st}: no swing fits")
        main_len = length(flat)
        ey = fy - main_len * MAIN_SLOPE
        ny = ey - 8.5
        main = heights(flat, fy, ey)
        # The main line hops once, in the middle of its middle straight.
        mid_s = main_len / 2
        m1 = trim_to(main, 0.0, mid_s - MAIN_HOP / 2)
        m2 = trim_to(main, mid_s + MAIN_HOP / 2, main_len)
        # Shortcut: straight past the other side, a speed gap in the middle.
        s_start, s_end = (fx, pz - side * SPLIT), (end_x, nz - side * SPLIT)
        sl = [(s_start[0] + (s_end[0] - s_start[0]) * k / 40, s_start[1] + (s_end[1] - s_start[1]) * k / 40) for k in range(41)]
        L = length(sl)
        sh = heights(sl, fy, ey)
        g0 = L * 0.55 - GAP[0] / 2
        s1 = trim_to(sh, 0.0, g0)
        s2 = trim_to(sh, g0 + GAP[0], L)
        # The landing ramp after the gap sits a little lower (you fall over it),
        # easing back to the shared end.
        l2 = length([(p[0], p[2]) for p in s2])
        acc, out2 = 0.0, []
        for k, p in enumerate(s2):
            if k:
                acc += math.dist((s2[k - 1][0], s2[k - 1][2]), (p[0], p[2]))
            out2.append((p[0], p[1] - GAP[1] * (1 - acc / l2), p[2]))
        s2 = out2
        names = NAMES[st]
        ramps.append((st, "Approach", names[0], dive))
        ramps.append((st, "Main", names[1], m1))
        ramps.append((st, "Main", names[2], m2))
        ramps.append((st, "Shortcut", names[3], s1))
        ramps.append((st, "Shortcut", names[4], s2))
        gaps.append((st, 3))
        y = ny
    pads.append(("finish", FINISH[0], y, FINISH[1], 16.0))
    return pads, ramps, gaps


NAMES = {
    10: ("Off the overpass", "Highway sweep", "Interchange curve", "Median run", "Median leap"),
    11: ("Lobby drop", "Round the tower", "Plaza bend", "Tower gap run", "Skybridge leap"),
    12: ("Atrium dive", "Mall swing", "Food court curve", "Concourse run", "Skylight leap"),
    13: ("Furnace drop", "Stack sweep", "Slag bend", "Belt run", "Ladle leap"),
    14: ("Pour drop", "Round the Great Furnace", "Crucible curve", "Crane run", "Furnace leap"),
    15: ("Last drop", "Cooling sweep", "Gantry bend", "Gauntlet run", "Final leap"),
}


def fmt(p):
    return f"({p[0]:.2f}, {p[1]:.2f}, {p[2]:.2f})"


if __name__ == "__main__":
    pads, ramps, gaps = build()
    for p in pads:
        print(p)
    for st, role, name, pts in ramps:
        l = length([(q[0], q[2]) for q in pts])
        print(st, role, name, f"{l:.0f} m", f"slope {(pts[0][1]-pts[-1][1])/max(l,1):.3f}", len(pts))


# ---- writing the data files ------------------------------------------------

OG = "crates/game/assets/data/side/surf_overgrown.ron"
FD = "crates/game/assets/data/side/surf_foundry.ron"
OLD_PADS = [(3200.0, -905.0), (3640.0, -947.0), (4000.0, -1015.0), (4400.0, -1060.0),
            (4764.9, -1105.0), (5026.25, -1160.0), (5600.0, -1215.0)]


def delta(new_pads, x):
    """How far the course moved down at hall x (old pads to new)."""
    old = OLD_PADS
    new = [(p[1], round(p[2], 2)) for p in new_pads]
    def at(pts, x):
        if x <= pts[0][0]:
            return pts[0][1]
        for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
            if x <= x1:
                return y0 + (y1 - y0) * (x - x0) / (x1 - x0)
        return pts[-1][1]
    return at(new, x) - at(old, x)


def block(text, key, body):
    """Replace `key: [ ... ],` (the list, brackets balanced) with body."""
    i = text.index(f"    {key}: [")
    j = text.index("[", i)
    depth = 0
    for k in range(j, len(text)):
        if text[k] == "[":
            depth += 1
        elif text[k] == "]":
            depth -= 1
            if depth == 0:
                break
    end = text.index("\n", k) + 1
    return text[:i] + body + text[end:]


def shift_y(text, key, fields, new_pads):
    """In the list `key`, move each `field: v` (absolute heights) by the
    course's drop at that item's x."""
    i = text.index(f"    {key}: [")
    j = text.index("\n    ],", i)
    seg = text[i:j]
    out = []
    for line in seg.split("\n"):
        m = re.search(r"\bx: \(([-\d.]+), ([-\d.]+)\)", line) or re.search(r"\bx: ([-\d.]+)", line) \
            or re.search(r"\bat: \(([-\d.]+)", line)
        if m:
            xs = [float(g) for g in m.groups()]
            x = sum(xs) / len(xs)
            d = delta(new_pads, x)
            for f in fields:
                line = re.sub(rf"\b{f}: \(([-\d.]+), ([-\d.]+)\)",
                              lambda mm: f"{f}: ({float(mm.group(1)) + d:.1f}, {float(mm.group(2)) + d:.1f})", line)
                line = re.sub(rf"\b{f}: (-?[\d.]+)(?=[,)])",
                              lambda mm: f"{f}: {float(mm.group(1)) + d:.1f}", line)
            if "at: (" in line and "y" in fields:
                line = re.sub(r"\bat: \(([-\d.]+), ([-\d.]+), ([-\d.]+)\)",
                              lambda mm: f"at: ({mm.group(1)}, {float(mm.group(2)) + d:.1f}, {mm.group(3)})", line)
        out.append(line)
    return text[:i] + "\n".join(out) + text[j:]


def write():
    pads, ramps, gaps = build()
    r2 = lambda v: round(v, 2)
    padmap = {p[0]: p for p in pads}
    # ---- overgrown
    s = open(OG).read()
    body = "    pads: [\n" + "".join(
        f"        (stage: {st}, at: ({x:.1f}, {r2(y):.2f}, {z:.1f}), size: ({sz:.1f}, 1.0, {sz:.1f})),\n"
        for st, x, y, z, sz in pads if st in (10, 11, 12)) + "    ],\n"
    s = block(s, "pads", body)
    p13 = padmap[13]
    s = re.sub(r"standin: \(stage: 13, at: \([^)]*\)", f"standin: (stage: 13, at: ({p13[1]:.1f}, {r2(p13[2]):.2f}, {p13[3]:.1f})", s)
    body = "    ramps: [\n"
    for st, role, name, pts in ramps:
        if st > 12:
            continue
        body += f"        (stage: {st}, name: \"{name}\", calm: false, points: [{', '.join(fmt(p) for p in pts)}]),\n"
    s = block(s, "ramps", body + "    ],\n")
    s = re.sub(r"    speed_gaps: \[[^\]]*\],", "    speed_gaps: [" + ", ".join(f"({a}, {b})" for a, b in gaps if a <= 12) + "],", s)
    lows = [(p[0], p[1]) for st, role, name, pts in ramps if st <= 12 for p in pts]
    def terrace(m):
        x0, x1 = float(m.group(1)), float(m.group(2))
        low = min(y for x, y in lows if x0 - 30 <= x <= x1 + 30)
        return f"x: ({m.group(1)}, {m.group(2)}), z: ({m.group(3)}), top: {low - 40.0:.1f}"
    s = re.sub(r"x: \(([-\d.]+), ([-\d.]+)\), z: \(([^)]*)\), top: [-\d.]+", terrace, s)
    tops = [(float(a), float(b), float(t)) for a, b, t in
            re.findall(r"x: \(([-\d.]+), ([-\d.]+)\), z: \([^)]*\), top: ([-\d.]+)", s)]
    def fall(m):
        x = float(m.group(1))
        top = next(t for a, b, t in tops if abs(b - x) < 1)
        bottom = next(t for a, b, t in tops if abs(a - x) < 1)
        return f"x: {m.group(1)}, z: ({m.group(2)}), top: {top:.1f}, bottom: {bottom:.1f}"
    s = re.sub(r"x: ([-\d.]+), z: \(([^)]*)\), top: [-\d.]+, bottom: [-\d.]+", fall, s)
    for key, fields in [("decks", ["y"]),
                        ("loops", ["y"]), ("gantries", ["beam_y"]), ("halls", ["water_y", "roof_y", "balcony_y"])]:
        s = shift_y(s, key, fields, pads)
    open(OG, "w").write(s)
    # ---- foundry
    s = open(FD).read()
    s = re.sub(r"    pads: \[[^\]]*\],", "    pads: [" + ", ".join(
        f"({x:.2f}, {r2(y):.2f}, {z:.2f})" for st, x, y, z, sz in pads if st in (13, 14, 15)) + "],", s)
    f = padmap["finish"]
    s = re.sub(r"    finish: \([^)]*\),", f"    finish: ({f[1]:.2f}, {r2(f[2]):.2f}, {f[3]:.2f}),", s)
    body = "    ramps: [\n"
    for st, role, name, pts in ramps:
        if st < 13:
            continue
        body += f"        (stage: {st}, role: {role}, points: [{', '.join(fmt(p) for p in pts)}]),\n"
    s = block(s, "ramps", body + "    ],\n")
    s = re.sub(r"    speed_gaps: \[[^\]]*\],", "    speed_gaps: [" + ", ".join(f"({a}, {b})" for a, b in gaps if a >= 13) + "],", s)
    low = r2(f[2])
    s = re.sub(r"    floor: [-\d.]+,", f"    floor: {low - 81.0:.1f},", s)
    s = re.sub(r"    sea: [-\d.]+,", f"    sea: {low - 57.0:.1f},", s)
    # The hall's walls: only 20 m over the foundry's first pad (you drop into
    # the works, not into a shed).
    s = re.sub(r"(walls: \(north: [-\d.]+, south: [-\d.]+, thick: [-\d.]+, top: )[-\d.]+",
               lambda m: f"{m.group(1)}{r2(padmap[13][2]) + 20.0:.1f}", s)
    s = place_foundry(s, [pts for st, role, name, pts in ramps if st >= 13])
    open(FD, "w").write(s)
    print("written")


def dense(lines, step=3.0):
    out = []
    for pts in lines:
        for a, b in zip(pts, pts[1:]):
            n = max(1, math.ceil(math.dist((a[0], a[2]), (b[0], b[2])) / step))
            for k in range(n + 1):
                out.append((a[0] + (b[0] - a[0]) * k / n, a[2] + (b[2] - a[2]) * k / n))
    return out


def place_foundry(s, lines):
    """Keep the foundry's dressing clear of the new lines (the rules of
    `the_set_dressing_stays_clear_of_the_ramps`, with a couple of metres to
    spare), adding replacements on a seeded scan until each kind is back to
    its count, and put the Great Furnace where stage 14's lines are furthest."""
    smp = dense(lines)
    dist = lambda x, z: min(math.hypot(x - a, z - b) for a, b in smp)
    used = []
    def free(x, z, r):
        return all(math.hypot(x - a, z - b) > r + rb + 6 for a, b, rb in used)
    cands = [(4380.0 + 37.0 * ((i * 7) % 34), -2090.0 + 23.0 * ((i * 11) % 18)) for i in range(34 * 18)]

    def redo(key, parse, ok, fmt_item, size, make):
        nonlocal s
        i = s.index(f"    {key}: [")
        j = s.index("    ],", i)
        items = [parse(m) for m in re.findall(r"\(([-\d., ]+)\)", s[i:j])]
        n = len(items)
        keep = [it for it in items if ok(it) and free(it[0], it[1], size(it))]
        for it in keep:
            used.append((it[0], it[1], size(it)))
        k = 0
        while len(keep) < n:
            x, z = cands[(k * 13 + len(key) * 5) % len(cands)]
            k += 1
            if k > 5000:
                break
            it = make(x, z, items[len(keep) % len(items)])
            if ok(it) and free(x, z, size(it)):
                keep.append(it)
                used.append((x, z, size(it)))
        head = s[i:s.index("\n", i) + 1]
        s = s[:i] + head + "".join(f"        ({fmt_item(it)}),\n" for it in keep) + s[j:]

    nums = lambda m: [float(v) for v in m.split(",")]
    # The Great Furnace first: the spot of stage 14 furthest from any line.
    best = max(((x, z) for x in range(4900, 5100, 2) for z in range(-2002, -1940, 2)), key=lambda p: dist(*p))
    gd = dist(*best)
    rad = max(8.0, min(18.0, gd - 17.0))
    s = re.sub(r"great: \(at: \([^)]*\), radius: [\d.]+",
               f"great: (at: ({best[0]:.1f}, {best[1]:.1f}), radius: {rad:.1f}", s)
    used.append((best[0], best[1], rad + 10))
    redo("stacks", nums, lambda it: dist(it[0], it[1]) > it[2] + 32,
         lambda it: f"{it[0]:.1f}, {it[1]:.1f}, {it[2]:.1f}, {it[3]:.1f}", lambda it: it[2],
         lambda x, z, o: [x, z, o[2], o[3]])
    redo("towers", nums, lambda it: dist(it[0], it[1]) > 24 * it[2] + 22,
         lambda it: f"{it[0]:.1f}, {it[1]:.1f}, {it[2]:.1f}", lambda it: 24 * it[2],
         lambda x, z, o: [x, z, o[2]])
    redo("gears", nums, lambda it: dist(it[0], it[1]) > 14 + it[2],
         lambda it: f"{it[0]:.1f}, {it[1]:.1f}, {it[2]:.1f}, {it[3]:.1f}, {it[4]:.1f}", lambda it: 4 * it[2],
         lambda x, z, o: [x, z, o[2], o[3], o[4]])
    redo("pistons", nums, lambda it: dist(it[0], it[1]) > 16,
         lambda it: f"{it[0]:.1f}, {it[1]:.1f}", lambda it: 3.0, lambda x, z, o: [x, z])
    redo("belts", nums,
         lambda it: all(dist(it[0] - it[2] / 2 + it[2] * k / 4, it[1]) > 22 for k in range(5)),
         lambda it: f"{it[0]:.1f}, {it[1]:.1f}, {it[2]:.1f}", lambda it: it[2] / 2,
         lambda x, z, o: [x, z, o[2]])
    # Gantries span the middle line (z -1935): legs clear of every line.
    i = s.index("    gantries: [")
    j = s.index("    ],", i)
    gs = [nums(m) for m in re.findall(r"\(([-\d., ]+)\)", s[i:j])]
    ok = lambda x, h: all(dist(x, -1935 + sd * h) > 18 for sd in (-1, 1))
    keep = [g for g in gs if ok(g[0], g[1])]
    x = 4410.0
    while len(keep) < len(gs) and x < 5560:
        if ok(x, 26.0) and all(abs(x - g[0]) > 30 for g in keep):
            keep.append([x, 26.0, 36.0])
        x += 7.0
    keep.sort()
    s = s[:i] + "    gantries: [\n" + "".join(f"        ({g[0]:.1f}, {g[1]:.1f}, {g[2]:.1f}),\n" for g in keep) + s[j:]
    print("great furnace", best, round(gd, 1), "radius", rad)
    return s


if __name__ == "__main__" and "--write" in sys.argv:
    write()
