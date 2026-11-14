# Screenshot goldens (S3)

Deterministic captures of the key places, compared with a pixel tolerance so
world or rendering changes show up as a failing `--check` instead of quietly
drifting. Generated and compared **on Linux with Mesa's software Vulkan
(lavapipe) only** — `linux-lavapipe/` is the golden set. macOS renders
differ (Metal); `dev-macos/` captures are development reference only and are
never compared.

## Regenerating

On a Linux machine (or CI) with `xvfb`, `mesa-vulkan-drivers` and
`libxkbcommon-x11-0` installed:

```sh
git lfs pull                                  # pointer stubs make blank shots
scripts/goldens.sh --update                   # capture into linux-lavapipe/
scripts/goldens.sh --check                    # capture fresh and compare
scripts/goldens.sh --check-only               # re-compare the last captures
```

The shot list is `scripts/golden_shots.txt` (one row per golden: name, start,
scene, camera, warehouse seed, wait). Pins every run uses: 1280x720 (the UI
golden is 1920x1080), `FRIENDSLOP_INPUT=keyboard`, `FRIENDSLOP_LOW=0` (the
High preset), `WGPU_BACKEND=vulkan`, 12 s settle, warehouse floor plan seed 7.
Time of day is pinned with `FRIENDSLOP_TIME` and world animation freezes
(`screenshot::frozen()`) while a capture runs.

## Comparing (`scripts/golden_compare.py`)

Per channel, differences of 2/255 or less are noise and count as zero. A pair
**fails** when either holds:

- more than **1.5%** of pixels have some channel differing by more than
  **8/255**, or
- the RMSE over all channels (after the deadzone) is above **3.0**.

Missing, unreadable, size-mismatched or blank captures fail outright. On
failure an amplified diff heatmap is written (`.diff.png`, under
`--diff-dir`), and the script exits non-zero with one line per shot.

Optional foliage masks: `masks/<name>.png`, same size as the golden, black
pixels ignored (foliage sway/wind can't be frozen perfectly). Add one only
when a shot is flaky for a known animated region — masks are a scalpel, not
a rug.

## `VERSIONS.txt`

Every `--update` appends one line
(`date-utc git-sha os backend`), so a golden's provenance is one `git log
-L` away. A capture wave expected to change the pictures (renderer PRs, asset
re-exports) should say so in its PR and re-run `--update` afterwards.

Note: open renderer PRs (#397, #399) change how frames look — generate the
first golden set **after** they land, or expect one regeneration.
