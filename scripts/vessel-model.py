#!/usr/bin/env python3
"""Rebuild the Vessel's model in the modeler from the game's own boxes.

The Vessel's geometry lives in `crates/side-nyc/src/features/vessel.rs`
(`pieces()`): its colliders and its model are the same boxes, so what you see
is what you walk on. This dumps them, builds them in the modeler as box
primitives grouped by material (v_steps, v_copper, v_deck) on the `nyc-vessel`
document, saves the prefab and exports `models/prefabs/nyc-vessel.gltf`.

    scripts/vessel-model.py            # dump, build, save, export
    scripts/vessel-model.py --dry-run  # only write the calls file

Needs the modeler (`~/Code/modeler/bin/modeler/mcp_driver.py`, or MODELER_DRIVER).
"""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GAME = ROOT / "crates/game"
DRIVER = os.environ.get(
    "MODELER_DRIVER", str(Path.home() / "Code/modeler/bin/modeler/mcp_driver.py")
)
DOC = "nyc-vessel"
# Each part of the model and the procedural material it wears.
PARTS = {
    "Steps": ("v_steps", "vs-steps"),
    "Copper": ("v_copper", "vs-copper"),
    "Deck": ("v_deck", "nyc-granite"),
}


def dump_boxes(path):
    env = dict(os.environ, VESSEL_BOXES=str(path))
    subprocess.run(
        ["cargo", "test", "-q", "-p", "friendslop-side-nyc", "write_model_boxes", "--", "--ignored"],
        cwd=ROOT,
        env=env,
        check=True,
    )
    return json.loads(Path(path).read_text())


def materials():
    """The three materials, as the current source keeps them (each part
    carries its material's node graph)."""
    prefab = json.loads((GAME / ".modeler/prefabs" / f"{DOC}.json").read_text())
    out = {}
    for part in prefab["parts"]:
        pbr = part.get("material", {}).get("pbr")
        if pbr:
            out[pbr["material"]] = pbr["graph"]
    return out


def build_calls(boxes, graphs):
    calls = [{"name": "new_model", "args": {"name": DOC}}]
    for _, pbr in PARTS.values():
        g = graphs[pbr]
        calls.append(
            {
                "name": "material_ops",
                "args": {
                    "material": pbr,
                    "domain": g.get("domain"),
                    "tile": g.get("tile"),
                    "resolution": g.get("resolution"),
                    "ops": g["nodes"],
                },
            }
        )
    ops, members = [], {k: [] for k in PARTS}
    for i, b in enumerate(boxes):
        name = f"vb_{b['part'].lower()}_{i}"
        members[b["part"]].append(name)
        ops.append(
            {
                "type": "primitive",
                "name": name,
                "kind": "box",
                "size": [round(s, 4) for s in b["size"]],
                "translation": [round(c, 4) for c in b["at"]],
                "rotation": [round(c, 6) for c in b["rotation"]],
            }
        )
    build = [{"type": "group", "name": PARTS[p][0], "inputs": members[p]} for p in PARTS]
    build += [{"type": "hide_part", "part": n} for names in members.values() for n in names]
    build += [{"type": "uvs", "part": group, "projection": "metric"} for group, _ in PARTS.values()]
    look = [{"type": "material", "name": group, "pbr": pbr} for group, pbr in PARTS.values()]
    calls += [
        {"name": "apply_ops", "args": {"ops": ops}},
        {"name": "apply_ops", "args": {"ops": build}},
        {
            "name": "apply_ops",
            "args": {"ops": look, "expect": {"parts": [g for g, _ in PARTS.values()], "materials": "pbr"}},
        },
    ]
    return calls


def save_calls():
    return [
        {"name": "history_prune", "args": {"keep": 1}},
        {
            "name": "save_prefab",
            "args": {
                "force": True,
                "description": "The Vessel at Hudson Yards: 16 levels of 5 landings, 154 flights on "
                "inner and outer lanes, 46 m tall. Built by scripts/vessel-model.py from "
                "vessel.rs pieces().",
                "tags": ["nyc", "landmark", "stairs"],
            },
        },
        {
            "name": "export_model",
            "args": {"path": "models/prefabs/nyc-vessel.gltf", "optimize": {"preset": "game", "merge": "by_material"}},
        },
        {"name": "scene_summary", "args": {}},
    ]


def run(calls, tmp, name):
    path = Path(tmp) / f"{name}.json"
    path.write_text(json.dumps(calls))
    out = subprocess.run(
        [sys.executable, DRIVER, "--connect", "--doc", DOC, "--json", "--calls", str(path)],
        cwd=GAME,
        capture_output=True,
        text=True,
    )
    ok = out.returncode == 0
    for line in out.stdout.splitlines():
        print(line[:600])
        try:
            ok = ok and json.loads(line).get("ok", True)
        except json.JSONDecodeError:
            pass
    if not ok:
        sys.exit(f"{name} failed: nothing saved or exported\n{out.stderr[-2000:]}")


def main():
    dry = "--dry-run" in sys.argv
    with tempfile.TemporaryDirectory() as tmp:
        boxes = dump_boxes(Path(tmp) / "boxes.json")
        print(f"{len(boxes)} boxes")
        build = build_calls(boxes, materials())
        if dry:
            keep = ROOT / "target/vessel-calls.json"
            keep.write_text(json.dumps(build + save_calls()))
            print(f"calls in {keep}")
            return
        run(build, tmp, "build")
        run(save_calls(), tmp, "save")


if __name__ == "__main__":
    main()
