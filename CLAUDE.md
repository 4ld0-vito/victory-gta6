# CLAUDE.md (`victory-gta6`)

See [`AGENTS.md`](AGENTS.md) for the complete repository workflow, rules, build/test commands, benchmark preset, and stacked PR stack on `4ld0-vito/victory-gta6`.

Mandatory reading before making changes:
1. [`docs/asset-policy.md`](docs/asset-policy.md) — **Zero game data online** (`cargo xtask policy` before every commit).
2. [`docs/perf-handoff.md`](docs/perf-handoff.md) — Current FPS progression, `copystat` bottleneck profile, verified dead-ends, code map, and ready-to-execute Phase 8 implementation plan toward 30+ fps.

If running from the parent workspace folder, check the parent `ALLAPOT.md` and `AGENTS.md` first (the user expects replies in Hungarian there).
