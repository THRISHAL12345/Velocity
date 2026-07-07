# AGENTS.md — Velocity Runtime (v1)

**Project codename:** Velocity
**Document version:** 2.0 (v1 scope)
**Status:** Active build spec
**Owner:** Thrishal Doma
**Supersedes:** v0 spec (v0.1.0 released — runtime core + baseline benchmark, hypothesis partially validated)
**Scope:** This document is the single source of truth for building v1 of the Velocity runtime. It assumes the v0 codebase already exists and builds directly on top of it. Any AI coding agent (Claude Code, Cursor, etc.) or human contributor should be able to implement v1 entirely from this file plus the existing v0 repo.

---

## 1. Where v1 Picks Up

v0 shipped a real, working runtime and a fair, reproducible benchmark. It proved the runtime beats LangGraph by up to 2.2x at p99 under load. It also surfaced two honest findings that limited the full 5–10x hypothesis from holding:

1. **Bounded worker pools lose to unbounded Python coroutines at extreme concurrency** — at concurrency=1000, raw MCP (no resource cap) beat Velocity's fixed 64-worker pool, because 5,000 simultaneous tool-call requests queued against only 64 warm workers.
2. **Binary protocol savings are invisible when tool I/O dominates** — the codec saves <5μs per call, but the v0 mock tools sleep 5–50ms, so the saving is a rounding error against ~90ms of cumulative I/O per task.

v1 exists to close both gaps with real engineering, not by re-framing the benchmark to hide them. A third improvement — giving the baseline a fair resource cap — exists to make the eventual win defensible under scrutiny rather than an artifact of comparing a capped system to an uncapped one.

**v1 non-goals (still out of scope):** production deployment tooling, SDK/client libraries, multi-region distribution, real LLM-in-the-loop benchmarking, generalized plugin architecture for arbitrary tools. v1 is still a benchmark-driven runtime project, not a shipped product.

---

## 2. v1 Scope — Three Workstreams

| # | Workstream | Answers |
|---|---|---|
| 1 | **Elastic worker pool** | Is pool size actually the bottleneck, and is there a size where Velocity beats raw MCP outright? |
| 2 | **Low-latency task profile (`hft_tick`)** | Does the 5–10x hypothesis hold in the domain it was written for (sub-millisecond tools)? |
| 3 | **Fair-capped baseline** | Does Velocity still win once the baseline plays by the same resource-limit rules? |

Each workstream produces its own chart and its own paragraph in the report. None of them are allowed to replace or hide the v0 numbers — v1's report appends to the v0 story, it doesn't overwrite it.

---

## 3. Workstream 1 — Elastic Worker Pool

### 3.1 Problem restated

v0's `WorkerPool` has a fixed size (default 16, benchmark default 64) set at construction. Under concurrency=1000, this causes queuing that dominates the latency numbers — the bottleneck being measured is queue wait time, not runtime overhead.

### 3.2 Requirements

- Add a `pool_size` parameter to the benchmark CLI (`velocity-bench --pool-sizes 64,256,1024,4096`) that runs the **existing** `process_order` task at a **fixed concurrency of 1000** across each listed pool size, holding everything else constant.
- The worker pool implementation itself does not need to become dynamically self-resizing for v1 — a correctly *parameterized* fixed-size pool, benchmarked across multiple sizes, is sufficient to answer the question in §2. (True elastic/adaptive resizing, i.e., the pool growing and shrinking at runtime in response to load, is a valid v2 follow-up but is explicitly NOT required for v1 — don't build it unless the fixed-size sweep proves inconclusive.)
- Ensure the pool can actually be constructed at 4096 workers per tool type without unreasonable startup cost — profile pool construction time at each size and record it; if construction time itself becomes a confound (e.g., >500ms to spin up 4096×3 workers), that's a finding to report, not a bug to silently work around.
- New benchmark output: for each pool size, record p50/p95/p99 task completion time and worker-pool queue wait time specifically (time spent in `acquire()` before a worker is returned) as a separate metric — this isolates "pool contention" from "actual execution time," which is the whole point of the experiment.

### 3.3 Required interface additions

```rust
// velocity-bench/src/task_definitions.rs
pub struct PoolSweepConfig {
    pub pool_sizes: Vec<usize>,
    pub fixed_concurrency: usize,   // 1000, per spec
    pub warmup_iterations: usize,
    pub measured_iterations: usize,
}

pub async fn run_pool_sweep(config: PoolSweepConfig) -> Vec<PoolSweepResult>;

pub struct PoolSweepResult {
    pub pool_size: usize,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub avg_queue_wait_us: u64,
    pub pool_construction_ms: u64,
}
```

```rust
// velocity-core/src/worker_pool.rs — instrumentation addition only, no behavioral change required
impl WorkerPool {
    // acquire() must now record time-to-acquire internally and expose it via stats(),
    // so the bench harness can separate "waiting for a worker" from "worker doing the tool call."
    pub fn stats(&self) -> PoolStats; // extend existing struct with avg_wait_us field
}
```

### 3.4 Output

- `results/raw/pool_sweep_<size>.json` — one file per pool size tested
- A new chart: `results/graphs/pool_size_vs_latency.png` — x-axis pool size (log scale: 64, 256, 1024, 4096), y-axis p99 latency (log scale), with a horizontal reference line marking raw MCP's p99 at concurrency=1000 from the v0 results, so the crossover point (if any) is visually obvious.
- A new paragraph in `results/report.md`, computed programmatically (per the v0 fix — no hardcoded prose): state the pool size at which Velocity's p99 first drops below raw MCP's p99, or state plainly that no tested size crossed over, with the queue-wait breakdown as supporting evidence either way.

### 3.5 Acceptance criteria

- Sweep runs via `./scripts/run_all_benchmarks.sh --with-pool-sweep` (additive flag; default run behavior unchanged) or an equivalent dedicated script `scripts/run_pool_sweep.sh`.
- Report states, in one sentence generated from real numbers, whether a crossover pool size exists and what it is. No editorializing beyond what the numbers show.

---

## 4. Workstream 2 — Low-Latency Task Profile (`hft_tick`)

### 4.1 Problem restated

The `process_order` task's mock tools sleep 5–50ms. That's a fine proxy for a typical web-app agent, but it's the wrong domain for testing Velocity's actual thesis (agent infra for latency-sensitive systems: trading, robotics, real-time voice). At that delay scale, no orchestration layer's overhead is visible — you're measuring `sleep()`, not the runtime.

### 4.2 Requirements

- Add a second, fully independent task definition, `hft_tick`, structurally similar to `process_order` (a small dependency graph with both independent and dependent steps — reuse the same 5-step shape for comparability) but with tool delays reduced to **50–500 microseconds**, jittered, matching the "sub-millisecond tool" domain described in the pitch.
- Implement matching low-latency mock tools: `mock_memory_lookup` (in-memory hashmap read, ~50–150μs), `mock_calc_engine` (a small deterministic computation simulating a pricing/risk calculation, ~100–300μs), `mock_state_write` (in-memory state mutation, ~50–200μs). These replace `mock_db`/`mock_http`/`mock_file` for this task only — the original three tools and `process_order` task remain unchanged and continue to be benchmarked as-is.
- These new tools must be implemented identically (same delay distributions) across Velocity, the LangGraph baseline, and the raw MCP baseline — same fairness requirement as v0 §7.4/§8.
- Run the full existing concurrency sweep (1, 10, 100, 1000) against `hft_tick` for all three contenders, exactly as v0 did for `process_order`.

### 4.3 Required additions

```
crates/velocity-tools/src/
  ├── mock_memory_lookup.rs   (new)
  ├── mock_calc_engine.rs     (new)
  └── mock_state_write.rs     (new)

crates/velocity-bench/src/task_definitions.rs
  // add hft_tick task graph alongside existing process_order

baselines/langgraph_baseline/tools.py    // add matching Python implementations
baselines/raw_mcp_baseline/server.py     // add matching Python implementations
```

### 4.4 Output

- Results recorded and reported exactly like `process_order` — same JSON schema, same table format, tagged by task name so both task profiles coexist in one report without confusion.
- New bar charts per concurrency level for `hft_tick`, analogous to the existing `bar_chart_c*.png` set — name them distinctly (`bar_chart_hft_c1.png`, etc.) so v0's charts aren't overwritten.
- New line chart: `latency_vs_concurrency_hft.png`, same style as the existing one, for direct visual comparison against the `process_order` version.
- Report must explicitly state, with real computed numbers, whether the 5–10x hypothesis holds under this profile — this is the single most important number v1 produces, since it's the direct test of the core thesis in its intended domain.

### 4.5 Acceptance criteria

- `hft_tick` tool delays are verified (via a quick unit test asserting sampled delays fall in the 50–500μs range) before any benchmark numbers are trusted — a task profile that accidentally sleeps in milliseconds again defeats the entire point.
- Both task profiles run from the same single `./scripts/run_all_benchmarks.sh` invocation (or a clearly documented additive flag) — v1 should not require a separate undocumented manual process to reproduce.

---

## 5. Workstream 3 — Fair-Capped Baseline

### 5.1 Problem restated

Raw MCP's win at concurrency=1000 in v0 partly reflects the fact that it has no concurrency limit at all — 1000 simultaneous `asyncio` coroutines each independently sleeping, with no queuing, no contention, and no bound on in-flight connections. No real production system runs with unbounded concurrent connections to a downstream dependency. This makes the v0 comparison at high concurrency slightly unfair in raw MCP's favor.

### 5.2 Requirements

- Add a second variant of the raw MCP baseline, `raw_mcp_baseline_capped`, that wraps all tool calls behind an `asyncio.Semaphore` (or equivalent bounded resource pool) with a configurable limit.
- Run this capped variant at the same pool sizes used in Workstream 1 (64, 256, 1024, 4096) so the two systems (Velocity and capped raw MCP) are compared at matching resource limits, not just matching task graphs.
- This variant must be clearly labeled as a distinct contender in all output (`raw_mcp_capped_64`, `raw_mcp_capped_256`, etc.) — never silently replace or average with the original uncapped raw MCP results. The original v0 uncapped baseline results remain in the report unchanged; the capped variant is additive context, not a replacement.

### 5.3 Required additions

```
baselines/raw_mcp_baseline_capped/
  ├── server.py          # same task logic as raw_mcp_baseline, wrapped with semaphore
  └── requirements.txt
```

```python
# server.py — core addition
semaphore = asyncio.Semaphore(pool_size)  # pool_size passed via CLI arg or env var

async def call_tool_capped(tool_fn, *args):
    async with semaphore:
        return await tool_fn(*args)
```

### 5.4 Output

- Extend the pool-size-vs-latency chart from Workstream 1 (§3.4) to include a third line: capped raw MCP at each pool size, alongside Velocity and (as a flat reference line) the original uncapped raw MCP number. This produces the single most important chart in v1 — three-way, apples-to-apples, at matching resource limits.
- Report paragraph, computed programmatically: state plainly whether Velocity beats capped raw MCP at any tested pool size, and at which one, exactly as required in §3.5.

### 5.5 Acceptance criteria

- Capped variant produces materially different (higher) latency than the uncapped original at concurrency=1000 for at least the smallest tested pool size (64) — this is a sanity check proving the semaphore is actually constraining concurrency and not a no-op.

---

## 6. Repository Structure Changes (delta from v0)

```
velocity/
├── crates/
│   ├── velocity-core/
│   │   └── src/worker_pool.rs        # MODIFIED: expose queue-wait stats
│   ├── velocity-tools/
│   │   └── src/
│   │       ├── mock_memory_lookup.rs # NEW
│   │       ├── mock_calc_engine.rs   # NEW
│   │       └── mock_state_write.rs   # NEW
│   └── velocity-bench/
│       └── src/
│           ├── task_definitions.rs   # MODIFIED: add hft_tick, PoolSweepConfig
│           └── pool_sweep.rs         # NEW
├── baselines/
│   ├── langgraph_baseline/
│   │   └── tools.py                  # MODIFIED: add hft_tick tools
│   ├── raw_mcp_baseline/
│   │   └── server.py                 # MODIFIED: add hft_tick tools
│   └── raw_mcp_baseline_capped/      # NEW
│       ├── server.py
│       └── requirements.txt
├── results/
│   ├── raw/
│   │   ├── pool_sweep_*.json         # NEW
│   │   └── hft_tick_*.json           # NEW
│   ├── graphs/
│   │   ├── pool_size_vs_latency.png  # NEW
│   │   ├── bar_chart_hft_c*.png      # NEW
│   │   └── latency_vs_concurrency_hft.png  # NEW
│   └── report.md                     # MODIFIED: append v1 sections, don't remove v0 sections
└── scripts/
    ├── run_pool_sweep.sh             # NEW (or additive flag on existing script — implementer's choice)
    └── run_all_benchmarks.sh         # MODIFIED: orchestrate v0 + v1 runs
```

---

## 7. Coding Standards (carried over from v0, unchanged)

- `cargo fmt` and `cargo clippy --all-targets -- -D warnings` must pass clean.
- No `unwrap()` in runtime hot paths.
- Every public function added in this spec requires a doc comment explaining latency-relevant behavior, consistent with v0's standard.
- Commit messages reference the workstream (e.g., `feat(pool): add configurable pool-size sweep per §3.2`).
- **New for v1:** any change to `results/report.md`'s generation logic must preserve the v0 sections unmodified in structure — v1 findings are appended, not blended into or replacing the v0 narrative. A reader should be able to see the full v0→v1 story in one document.

---

## 8. Testing Requirements (additions to v0)

- Unit test asserting `hft_tick` mock tool delays sample within the 50–500μs band (per §4.5).
- Unit test verifying `raw_mcp_baseline_capped`'s semaphore actually blocks beyond its configured limit (e.g., spin up `pool_size + 10` concurrent calls, assert no more than `pool_size` are in-flight at any instant).
- Integration test: `pool_sweep` at a small pool size (e.g., 4) and a small concurrency (e.g., 20) completes correctly and produces non-zero queue-wait time, proving the queue-wait instrumentation actually measures something.

---

## 9. Milestones

| Phase | Deliverables | Exit criteria |
|---|---|---|
| **Phase 1** | Workstream 1: pool-size sweep implemented, instrumented, benchmarked at 64/256/1024/4096 | `pool_size_vs_latency.png` exists with real data; report states crossover point or its absence |
| **Phase 2** | Workstream 2: `hft_tick` task + tools implemented across all 3 contenders, benchmarked at all 4 concurrency levels | Report states, with real numbers, whether 5–10x hypothesis holds under this profile |
| **Phase 3** | Workstream 3: capped raw MCP baseline implemented, benchmarked at same pool sizes as Phase 1 | Three-way pool-size chart exists; report states whether/where Velocity beats the fair-capped baseline |
| **Phase 4** | Report consolidation, README v1 section, tag `v1.0.0` | `results/report.md` contains v0 + all three v1 sections, internally consistent, regenerable from one script |

No fixed week estimates given — unlike v0, this is scoped by workstream completion, not calendar time, since Phase 2's result (does the hypothesis hold in the intended domain) may reasonably prompt scope changes to Phases 1 and 3 depending on what it shows.

---

## 10. Success Metrics — v1 Definition of Done

1. All three workstreams (§3, §4, §5) have real, checked-in raw data and generated charts — no workstream is reported on without backing data.
2. `results/report.md` answers, in plain sentences computed from real numbers (never hardcoded), all three of:
   - Is there a worker pool size at which Velocity beats raw MCP at concurrency=1000?
   - Does the 5–10x latency hypothesis hold under the `hft_tick` low-latency task profile?
   - Does Velocity beat raw MCP once raw MCP is given a matching resource cap?
3. The full v1 suite (v0 tasks + `hft_tick` + pool sweep + capped baseline) is runnable via a single documented command, per the reproducibility bar set in v0.
4. If any of the three answers above is "no" or "partially," the report says so as plainly as v0's did — v1's job is to close gaps with real engineering, not to reword the conclusion until it sounds better.

---

## 11. What Comes After v1 (context only, not in scope)

- True elastic/adaptive pool resizing at runtime (rather than a benchmarked fixed-size sweep)
- Real LLM-in-the-loop benchmarking, replacing the static task graph
- SDK/client libraries for adoption without a Rust rewrite
- Distributed/multi-region deployment

---

*End of spec. Where this document is ambiguous, resolve in favor of whatever keeps the v0→v1 comparison honest and reproducible — if a shortcut would make v1's numbers look better without actually being a fairer or faster system, it does not belong in this build.*
