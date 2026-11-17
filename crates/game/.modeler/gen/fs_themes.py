"""Generates the depth-themed copies of Fulton St's corridor tiles
(`fs-tile-<piece>-<theme>`) by swapping each prefab's materials. Run from crates/game."""
import json, copy
PIECES = ["closed", "end", "straight", "corner", "tee", "cross"]
# role -> material file; None keeps the original. Roles: wall, floor, ceiling,
# band (trim, bases), pillar, lamps.
THEMES = {
    "service":  dict(wall="nyc-concrete", floor="pk-concrete", ceiling="ns-iron", band="fs-galvanized", pillar="fs-pipe-green", lamps=None),
    "sewer":    dict(wall="zn-mossy-stonework", floor="pk-sewage", ceiling="tn-brick", band="ns-iron", pillar="zn-stonework", lamps="sf-lamp-green"),
    "liminal":  dict(wall="wd-checker", floor="pn-terrazzo", ceiling="pn-ceiling", band="bt-tiles", pillar="pk-porcelain", lamps="wh-tube"),
    "flesh":    dict(wall="wd-meat", floor="bt-skin", ceiling="wd-meat", band="bt-cheek", pillar="bt-eye", lamps="sf-lamp-red"),
    "hell":     dict(wall="sp-hellrock", floor="sp-hellrock", ceiling="sp-hellrock", band="fs-glow-red", pillar="sp-hellrock", lamps="sf-lamp-red"),
}
def role(n):
    if n.startswith("wall") or n == "block": return "wall"
    if n.startswith("band") or n == "bases": return "band"
    if n.startswith("pillar"): return "pillar"
    return {"floor": "floor", "ceiling": "ceiling", "lamps": "lamps"}.get(n)
for theme, mats in THEMES.items():
    graphs = {}
    for m in {v for v in mats.values() if v}:
        g = json.load(open(f".modeler/materials/{m}.json"))
        graphs[m] = {k: g[k] for k in ("domain", "tile", "resolution", "nodes", "params") if k in g}
    for piece in PIECES:
        d = json.load(open(f".modeler/prefabs/fs-tile-{piece}.json"))
        name = f"fs-tile-{piece}-{theme}"
        d["name"] = name
        for o in d["ops"]:
            mo = o.get("material")
            if not mo: continue
            r = role(mo["name"]); want = mats.get(r) if r else None
            if not want: continue
            mo["pbr"] = {"material": want, "scale": 1.0, "graph": copy.deepcopy(graphs[want])}
            mo["base_color"] = None
        json.dump(d, open(f".modeler/prefabs/{name}.json", "w"))
        print(name)
