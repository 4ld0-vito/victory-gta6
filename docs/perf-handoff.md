# Performance Handoff & Phase 8 Blueprint (`victory-gta6`)

**Goal:** Reach **30+ fps** (`<= 33.3 ms/frame`) in world gameplay on Series X profile (`RTX 4070 12 GB`, `i7-14700KF`, `32 GB RAM`) with **0 errors** and **RAM < 32 GB**.

---

## 1. Benchmark Progression

| Run / Branch | Profile / Key Flags | World FPS (heavy / peak) | Peak RAM | Notes / Findings |
|---|---|---|---:|---|
| Baseline (`fork1_diag`, upstream fork) | `diag`, default | 0.1–3.3 fps (`0.39` avg) | 18.8 GB | Texture hold-still (`843–2417 ms/frame`), copies `101 ms`, copy-back `313 ms` |
| Phase 6 (`speed12_p6_seriesx`, `bcc0465`) | `release`, Series X, preset | 8.5 fps (3,040 draws) / **13.0 fps** (537 draws) | 22.2 GB | `cbv_copy`, kernel fallbacks, `0.0 ms` upload settles, GPU `66–75%` busy |
| Phase 7 (`speed16_p7_1080p`, `0d0bc22`) | `release`, `1080p`, `GUARD_HOT=1` | 4.1–7.3 fps / **14.5 fps** | 21.7 GB | `heap buffers` down to `10–16 ms`, but `GUARD_HOT=1` doubled copies (`197–338 MB`) |
| **Phase 7b (`speed18_p7b_release`, `335192f`)** | `release`, Series X, `GUARD_HOT` default OFF | **5.9–8.2 fps** (3.1k–4.0k draws) / **16.4 fps** (`60.8 ms`, 583 draws) | **21.1 GB** | `0 errors`; `preparing 72.4–85.4 ms`, `copies 42.7–49.8 ms` (`116–138 MB`), `heap buffers 9.1–12.5 ms`, `copying back 45–57 ms`, GPU `65–70%` |
| Phase 7b diag (`speed19_p7b_diag`, `335192f`) | `diag` (`copystat`) | 4–8 fps (diag overhead) | 21.3 GB | Isolated exact CPU/GPU sinks (see Section 2) |
| Phase 7b barrier probe (`speed21_p7b_barriers_diag`, `335192f`) | `diag`, `VICTORY_BARRIER_TEST=both` (`experiment 3`) | 3.3–8.2 fps (1.4k–4.5k draws) / 11.8 fps (525 draws) | 21.3 GB | `0 errors`, `CPU 694 s`, `905 barrier_groups` + `2195 indirect` bounded/frame, GPU `46–69%` busy (`on the GPU` `15–40 ms`) |

---

## 2. Exact Bottleneck Profile (`speed19_p7b_diag` & `speed21_p7b_barriers_diag`)

From `copystat` on heavy world frames (`~58` submissions/frame, `~30,000` list calls/frame):

1. **`guards` (`16.3–23.0 ms/frame`, `~330–335` guard faults/frame):**
   `watch::writes_ex` currently holds `JOURNAL.write()` *while* calling `guard(a, b)` (`VirtualQuery`/`VirtualProtect`), serializing worker threads inside `wide_refresh`.
2. **~3,100–3,400 global NULL UAV barriers/frame (`830–1,018` `barrier_groups` + `1,601–2,439` `indirect` clamps):**
   - `indirect::bound` / `bound_ahead` emits a global NULL `UAV_BARRIER` + `COMMON -> UNORDERED_ACCESS` and `indirect::done` emits `INDIRECT_ARGUMENT -> COMMON` on every indirect command, plus `HostList` uses a single `indirect_args` buffer so `done` forces a full drain.
   - `draw::State::window` only caches a **single** `Window`, and is cleared unconditionally on every read-only `draw`, `draw_indexed`, and `dispatch_mesh` in between indirect draws.
3. **Hardware page faults during host refreshes (`7,592–17,920` faults/frame, `moves 16–25 ms`, `56–142` fallbacks):**
   Still the single largest CPU-copy overhead inside `13k–21k` refreshes/frame (`host_to_copy` / `safe_copy`).
4. **Other measurable sinks:**
   - `lists reset`: `12.7–15.4 ms/frame`
   - `heap buffers`: `9.1–12.5 ms/frame` (`335192f` release)
   - `copying back`: `45–57 ms/frame` (relay merge & card back)
   - `Step::GpuWrite` scans: `declared_writes` and `Host::card_before` iterate all `21,000+` `Step::Refresh` items in `Deferred::copies` just to find `Step::GpuWrite` entries.
   - Multi-thread list recording sum: `332–501 ms/frame` across 6 threads (`~13 ms` post-present gap before first list call).

---

## 3. Verified Dead-Ends (Do Not Repeat)

- **Xbox Series S (`--console series-s`):** Crashes ~85 s into world load with `DXGI_ERROR_INVALID_CALL (0x887A0001)` after FSR4 ACO fallback and a `Format 67` `dimension: 3` (`1920x1080`, `flags: 5`) RT+UAV refusal. Additionally, the D3D12 debug layer SDK is not installed on this machine (`0x887A002D`). **Stay on Series X.**
- **`VICTORY_GUARD_HOT=1`:** Leaving faulted 64 KiB blocks writable across a frame caused all ~63 submissions/frame to copy them unconditionally, doubling copied MB (`116–138 MB` -> `197–338 MB`). Keep default OFF (already done in `335192f`).
- **`VICTORY_INPLACE=1` (as-is in `inplace.rs`):** Reduced CPU (`1163 s` -> `873 s` in `speed15`) across `76 windows (348 MB)`, but GPU L0 reads slowed card execution enough to leave world FPS flat (`5.2–12.9 fps`). Only revisit with selective placement/caching if tackling page faults/copying directly.
- **Lowering display resolution alone (`1080p` / `720p`):** Did not raise heavy-frame FPS (`4.4–11.9 fps` at `720p`) because frames are CPU/sync bound while the GPU sits `58–70%` busy.

---

## 4. Hot Code Map (`crates/victory-gpu/src/`)

| File | Key Symbols | Role |
|---|---|---|
| `watch.rs` | `writes_ex`, `ask_once`, `ask_ahead`, `BlockRuns`, `Guards`, `guard`, `protect` | Write-watch page tracking, `VirtualProtect`/`VirtualQuery` coalescing, `JOURNAL` `RwLock` |
| `host.rs` | `Host`, `HostList`, `Deferred`, `card_before`, `declared_writes`, `indirect_scratch`, `cbv_scratch`, `reset` | Command list wrapping, deferred copies/refreshes, card-back relay, scratch pools |
| `indirect.rs` | `bound`, `bound_ahead`, `record_clamp_as`, `done`, `done_on`, `uav_barrier` | HLSL compute clamp of indirect draw/dispatch args & budgets |
| `draw.rs` | `State`, `Window`, `prepare`, `prepare_for`, `bind`, `dispatch`, `dispatch_mesh`, `draw`, `draw_indexed`, `execute_indirect`, `heap_buffers`, `heap_textures` | State translation, descriptor table allocation, indirect clamping window cache |
| `list.rs` | `barrier_groups`, `buffer_range`, `copy_buffer_region`, `copy_resource` | Intercepted title D3D12 command list methods |
| `shadow.rs` | `CardSpan`, `card_merge`, `card_merge_relay_locked`, `copy_stale`, `Shadow::placed` | Shadow buffer copies, card hashing, relay diff/merge |
| `descriptor/host.rs` | `TextureQueue::due_with_census`, `texture_slots_due`, `remapped` | Bindless descriptor queue & remapping handling |

---

## 5. Ready-to-Execute Phase 8 Blueprint (`perf/phase-8-indirect-gpu-sync`, PR #4)

Branch from `docs/agent-handoff` (or `perf/phase-7-guards-heap`). Five surgical changes:

### 1) `watch.rs` — Move `guard()` outside `JOURNAL.write()`
In `writes_ex` (both `if known_guard && guarded` and `if failed`):
- Call `guard(...)` **before** acquiring `JOURNAL.write()`, so worker threads in `wide_refresh` run `VirtualQuery`/`VirtualProtect` concurrently instead of serializing behind the exclusive lock.
- In `known_guard`, if `guarded_pages.is_empty()`, take only `JOURNAL.read()` when `j.asked > 0` instead of escalating to `JOURNAL.write()`.

### 2) `host.rs` — Dirty UAV tracking, 2x4 rotating indirect scratch pools, and `Deferred::gpu_writes`
- Add `uav_dirty: AtomicBool` (initialized `true` in `new_list` and reset `true` in `HostList::reset`) plus `mark_uav_dirty(&self)`, `clear_uav_dirty(&self)`, and `take_uav_dirty(&self) -> bool`.
- Replace `indirect_args: Mutex<Option<(ID3D12Resource, u64)>>` with 2x4 rotating ring slots (`IndirectPools`: `direct` ring for single commands, `window` ring for multi-draw windows), each slot tracking `(resource, size, in_indirect_state)`. Expose `indirect_scratch_slot(&self, size: u64, windowed: bool) -> Option<(ID3D12Resource, bool)>`.
  - When a slot's `in_indirect_state` is `true`, the caller transitions `INDIRECT_ARGUMENT -> UNORDERED_ACCESS` only for that slot, avoiding global drains while rotating away from `done`.
  - In `HostList::reset`, reset ring indices and mark all slot `in_indirect_state = false`.
- Add `gpu_writes: Vec<(Arc<Shadow>, usize, usize)>` to `Deferred` (populated in `defer_gpu_write`, cleared/bounded alongside `copies` in `Deferred::clear()`). Update `declared_writes` and `Host::card_before` to iterate `deferred.gpu_writes` instead of scanning 21,000+ `Step::Refresh` entries.

### 3) `indirect.rs` — Smart indirect barriers & no-op `done`
- Gate with `VICTORY_SMART_INDIRECT_BARRIERS` (default **on**; `=0` reverts to legacy behavior).
- Wire `indirect_scratch_slot(size, windowed)` into `bound` and `bound_ahead`:
  - Emit leading `uav_barrier` only when `waits && list.take_uav_dirty()` (or legacy when flag is `0`).
  - Pass `prev_in_indirect` into `record_clamp_as` so the pre-clamp transition uses `D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT` when rotating into an already-used slot instead of `COMMON`.
- Make `indirect::done` / `done_on` a no-op under smart barriers (the next `ExecuteIndirect` already expects `INDIRECT_ARGUMENT` from `record_clamp_as`'s post-barrier, and slot rotation keeps prior `ExecuteIndirect` reads safe).
- Update/add unit tests (`warp_rotating_indirect_scratch_and_elided_uav_barriers`) alongside `host_list_reuses_and_grows_scratch_across_submissions` and run `cargo test -p victory-gpu`.

### 4) `draw.rs` — `uav_dirty` marking & 4-entry `windows: Vec<Window>`
- Call `list.mark_uav_dirty()` whenever a shader can write (`dispatch`, `prepare_for` when `t.stores || !write_skip()`, storing `execute_indirect`).
- Upgrade `self.window: Option<Window>` to `windows: Vec<Window>` (capacity 4, MRU/FIFO) in `State` (and `State::replay` if copied/cleared).
  - Do **not** invalidate `windows` on read-only `draw`, `draw_indexed`, or `dispatch_mesh`; only clear `windows` on shader writes (`dispatch`, `writes` in `prepare_for`), `!windowed`, or `list.first_use(WINDOW_USE)`.
  - Widen the stride/offset walk match check to `step <= 1024`.

### 5) `list.rs` — Sync `barrier_groups` with `uav_dirty` & inspect title barrier payloads (`diag`)
- After `barrier_groups` emits `UAV_BARRIER`, call `list.clear_uav_dirty()` so an immediately following indirect clamp does not emit a redundant second NULL UAV barrier.
- Under `#[cfg(feature = "diagnostics")]`, log the first 20 non-empty `barrier_groups` calls' `(Type, NumBarriers)` safely via `crate::watch::safe_copy` to verify whether title barriers are narrower than full UAV flushes.

### Phase 8 Verification Checklist
1. `cargo xtask policy` -> `asset policy: ... files ok`
2. `cargo test -p victory-gpu` -> `0 failed`
3. `cargo build --release` and `cargo build --profile diag -p victory --features diagnostics`
4. Benchmark `release` (210 s) + `diag` (150 s) on Series X preset; target **0 errors** and check improvement from `16.4 fps peak / 5.9–8.2 fps heavy`.
5. Commit, push to `mine`, open **PR #4** on `4ld0-vito/victory-gta6`, and update parent `ALLAPOT.md`.

---

## 6. Phase 9+ Roadmap Toward 30+ FPS

After Phase 8 lands, remaining dominant costs from `speed19`/`speed21` are:
1. **Eliminate `7.5k–17.9k` page faults/frame (`moves 16–25 ms` + `142` fallbacks + `19.6 GB` scanned refreshes):** Investigate why `refresh` repeatedly faults unmapped/guarded spans (`copied from ... 36.8x/frame` hotspots in `copystat`) — e.g., per-frame pinned/validated source extents or selective section/in-place caching without slow L0 GPU reads.
2. **Cut `copying back 45–57 ms/frame`:** Profile `relay_card_back` / `card_merge_relay_locked` vs `VICTORY_CARD_BUFFERS_MB` & `VICTORY_RELAY` tuning.
3. **Cut `lists reset 12.7–15.4 ms/frame` & post-present stall (`12.9 ms`):** Pool allocator resets asynchronously / pre-reset off the critical thread.
4. **Black shadows (post-30 fps or parallel track):** `VICTORY_BVH_BUILDER` once playable framerate is reached.
