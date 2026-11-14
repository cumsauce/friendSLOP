# Perf budget (S9)

Runs: `FRIENDSLOP_PERF=3 FRIENDSLOP_START=<name> cargo run -- host`, batched
by `scripts/perf_sweep.sh`, at **1280x720, High preset** (`FRIENDSLOP_LOW=0`).
Rows are assembled from the runs' `perf:` lines in
`hardening/perf-sweep-raw.txt`; the first line of each run records the
graphics preset, resolution and `WGPU_BACKEND`.

**Budget rule**: median ≤ **16.6 ms** and p95 ≤ **33 ms** at 1280x720 High.
Record the reference hardware here at the first fill (`uname -m`, model).

| start | median/p95/worst ms | fps | draw calls | meshes | lights | tex VRAM | entities | verdict |
|---|---|---|---|---|---|---|---|---|
| fulton | | | | | | | | |
| penn_street | | | | | | | | |
| chambers_st | | | | | | | | |
| warehouse | | | | | | | | |
| forest_clearing | | | | | | | | |
| factory_town | | | | | | | | |

Notes:

- Runs happen in a later wave (the goldens/perf wave). This file owns the
  table header and the rule; fill rows from the sweep log.
- The six rows above are the predicted-worst starts (S9: fulton's 40 WFC
  levels, the NYC prefab sets at penn_street/chambers_st, the warehouse's
  shadow-casting interior lights, forest_clearing's canopy, factory_town's
  400k-lumen light); sweep the rest of the ~24 start points afterwards.
- Draw calls are the render-world count (binned batches + sorted items over
  every view) falling back to a (mesh, material) proxy — the log marks the
  fallback with `(proxy)`. Tex VRAM is the loaded-image estimate (mips
  included; shadow maps excluded).
- A row **fails** the budget when median > 16.6 ms or p95 > 33 ms. The
  verdict column says `pass` / `over` with one clause of why.
- `FRIENDSLOP_PERF=3` repeats the per-owner mesh/light/batch breakdown every
  30 s after the 25 s mark — that's what "which model owns the draws" is
  answered from (re-export many-part models with `merge: "by_material"`, a
  modeler task — see hardening/modeler-todo.md item 8).
