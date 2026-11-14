# Nightly test hardening

What landed on `claude/nightly-tests` (all in `crates/game/tests/it/`, one test binary):

- **`harness.rs`** — shared headless multiplayer harness. `peer()` builds one
  replicon test app with the game's `ProtocolPlugin`; `Sim::new(n)` spins up a
  listen server + n clients and steps until players replicate. Helpers:
  `step`, `run(secs)`, `until(secs, f)`, `join_late`, `disconnect(idx)`,
  `send(player, ev)`, `send_pair(a, b, ev_a, ev_b)` (same-tick batch),
  `count::<C>(Side::Server | Side::Client(i))`. Player 0 is the host on the
  server app; player n is `clients[n - 1]`. `step` skips kicked clients
  (exchanging with a disconnected test app panics — its `TestClientEntity` is
  removed by `disconnect_client`).
- **`protocol_order.rs`** + `protocol_hash.snapshot`, `protocol_order.snapshot`
  — pins the replicon `ProtocolHash` and the ordered `FeaturesProtocolPlugin`
  list from `src/features/mod.rs`, and asserts that list has no `#[cfg(`.
- **`protocol_cfg.rs`** — scans every `.rs` under `crates/*/src` and fails if
  `cfg(` sits within 3 lines of `replicate::<` / `add_*_message` /
  `add_*_event` (browser and desktop must register the identical protocol).
  Escape hatch: `protocol-cfg-allow` in a comment on either line.

## Running

```sh
CARGO_TARGET_DIR=<root>/target cargo test -p friendslop --test it
```

## Blessing the snapshots

Registration changes are intentional? Rewrite the snapshot files, review the
diff, commit:

```sh
BLESS=1 cargo test -p friendslop --test it protocol
```

Without `BLESS=1` a mismatch fails with: "protocol registrations changed; if
intentional run BLESS=1 cargo test -p friendslop --test it protocol".
