# Agent Guide (`victory-gta6`)

This repository is the `4ld0-vito` working fork (`mine`) of `fxceless/victory-gta6` (upstream `Marvisak/VIctory`), focused on making the target Xbox Series title run at **30+ fps playably** on Windows 11 (Intel i7-14700KF, NVIDIA RTX 4070 12 GB, 32 GB RAM).

> **Before coding or benchmarking:** Read [`docs/asset-policy.md`](docs/asset-policy.md) and [`docs/perf-handoff.md`](docs/perf-handoff.md).

## Current Status (`perf/phase-7-guards-heap` / `335192f`)

- **World performance (Series X release build, recommended flags):** **16.4 fps peak** (`60.8 ms/frame` at 583 draws), **5.9–8.2 fps** on heavy world frames (3.1k–4.0k draws), **0 errors**, **~21.1 GB peak Private RAM**.
- **Target:** **30+ fps** (`<= 33.3 ms/frame`) in gameplay with **0 errors/crashes** and **RAM < 32 GB**.

## Hard Rules

1. **Zero game data online (Rule #1):** Never commit, push, PR, or comment any game files, executable names/paths, local user paths, shader dumps/hashes, memory addresses from live runs, screenshots, or raw logs. Always run `cargo xtask policy` before committing.
2. **Push & PR only to `mine` (`4ld0-vito/victory-gta6`):**
   - `mine` (`https://github.com/4ld0-vito/victory-gta6`) is writable (`gh pr ... --repo 4ld0-vito/victory-gta6`).
   - `origin` (`fxceless/victory-gta6`) and `upstream` (`Marvisak/VIctory`) are **strictly off-limits** (no push, PR, or issue).
3. **One game process at a time:** Check `Get-Process victory -ErrorAction SilentlyContinue` before launching any run.
4. **No system modifications:** Do not change Windows drivers, pagefile, or firewall settings without explicit user permission.
5. **Stacked PR workflow:** Keep work cleanly separated on feature branches stacked on the previous PR branch and track every phase and benchmark result in a PR on `4ld0-vito/victory-gta6`.

## Build, Policy & Test Commands

Prepend Cargo to `PATH` if needed, and in Windows PowerShell 5.1 chain failures with `; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }` (PowerShell 5.1 does not support `&&`):

```powershell
# Verify asset policy before every commit
cargo xtask policy

# GPU crate unit tests (note: an occasional transient 0xc0000005 under parallel watch tests passes on immediate re-run)
cargo test -p victory-gpu

# Standard release binary -> target\release\victory.exe
cargo build --release

# Diagnostics binary (copystat, DRED, barrier counters) -> target\diag\victory.exe
cargo build --profile diag -p victory --features diagnostics
```

## Standard Series X Benchmark Preset

Always benchmark on `--console series-x` (default). On headless/non-interactive sessions use `VICTORY_KEYS="25:Enter,45:Enter"` to advance past the start screen cleanly.

```powershell
$env:VICTORY_KEYS="25:Enter,45:Enter"
$env:VICTORY_MEMORY_MAP="0"
$env:VICTORY_TEXTURE_NO_WAIT="1"
$env:VICTORY_INDIRECT_WINDOW="1"
$env:VICTORY_CARD_BUFFERS_MIN_KB="64"
$env:VICTORY_CARD_BUFFERS_MB="1280"
$env:VICTORY_CARD_WINDOWS_MB="1536"
$env:VICTORY_CARD_REHASH_RUNS="1"
$env:VICTORY_WIDE_REFRESH="1"
$env:VICTORY_WIDE_MERGE="1"
$env:VICTORY_WIDE_BACK="1"
$env:VICTORY_JOIN_COPIES="1"
$env:VICTORY_RELAY="2"
$env:VICTORY_LATE_WAIT_LISTS="512"
$env:VICTORY_HEAP_TEX_MS="2"
.\target\release\victory.exe run <game_dir>\<title_exe> --execute
```

Key log metric:
- `INFO victory_gpu::host: X fps, Y ms/frame (...), N draws/frame` (filter `N > 100` for world frames vs `10–12` menu draws).

## Branches & Stacked PRs (`4ld0-vito/victory-gta6`)

| Branch | Base | PR | Summary |
|---|---|---|---|
| `main` | — | — | Fork baseline (`578d459`) on top of upstream VIctory |
| `work` | `main` | PR #1 | Perf Phases 2–6 (`bcc0465`) |
| `perf/phase-7-guards-heap` | `work` | PR #2 | Phase 7 + 7b: `BlockRuns` guards, lazy `HeapCache`, O(N) `TextureQueue`, batched `card_before` (`335192f`) |
| `docs/agent-handoff` | `perf/phase-7-guards-heap` | PR #3 | Agent onboarding & Phase 8 technical blueprint (`docs/perf-handoff.md`) |
| `claude-zero-copy-sections` | — | — | Archived zero-copy committed-section experiment (`b9cd650`) |

## Next Handoff

Start the next performance PR (`perf/phase-8-indirect-gpu-sync`, PR #4 stacked on `docs/agent-handoff`) by following the ready-to-implement blueprint in [`docs/perf-handoff.md`](docs/perf-handoff.md).
