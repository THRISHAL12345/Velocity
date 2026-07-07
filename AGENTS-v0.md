# AGENTS.md — Velocity Runtime (v0 / MVP)

**Project codename:** Velocity
**Document version:** 1.0
**Status:** Active build spec
**Owner:** Thrishal Doma
**Scope:** This document is the single source of truth for building the v0 MVP of a low-latency agent tool-call execution runtime. Any AI coding agent (Claude Code, Cursor, etc.) or human contributor should be able to build the entire MVP from this file alone.

---

## 1. Executive Summary

Velocity is an execution runtime for AI agent tool-calling that replaces standard Python-based orchestration (LangGraph, raw MCP-over-stdio) with a purpose-built, systems-engineered layer. The runtime targets **5–10x lower tool-call round-trip latency** by eliminating three specific sources of overhead: JSON serialization cost, cold-start latency, and serial (non-overlapped) scheduling of LLM think-time against tool I/O.

The v0 deliverable is **not a product** — it is a runtime core plus a reproducible, honest benchmark proving or disproving the central latency claim, published as a public repo with a written report.

**Non-goals for v0:** multi-agent orchestration, persistent memory, planning/reasoning frameworks, UI, auth, multi-tenancy, billing, production deployment tooling. Anything not required to prove the latency claim is explicitly out of scope and must not be built.

---

## 2. Problem Statement

Standard agent tool-calling pipelines look like this:

```
LLM decides to call tool → serialize request (JSON) → dispatch to process/service
   → (possible cold start) → tool executes → serialize response (JSON)
   → deserialize on receipt → LLM resumes
```

Every hop above adds latency that is invisible in chat UX (a few hundred ms doesn't matter to a human reading text) but is fatal in latency-sensitive domains: algorithmic trading copilots, real-time voice agents, robotics control loops, real-time ad bidding, and interactive simulations/games.

No existing agent framework treats tool-call latency as a first-class systems problem. Velocity does.

---

## 3. Core Hypothesis (what this MVP must prove)

> A purpose-built runtime, applying (a) pre-warmed worker pools, (b) a binary wire protocol, and (c) an async scheduler that overlaps LLM think-time with tool I/O, reduces p99 tool-call round-trip latency by 5–10x versus LangGraph and raw MCP baselines, under both single-call and concurrent-load conditions.

Every component built in this spec exists to test this hypothesis. If a proposed feature does not serve this test, it does not belong in v0.

---

## 4. High-Level Architecture

```
+----------------------+
|   Agent Loop / LLM    |   (decides which tool to call, when)
+----------+-----------+
           | tool_call(name, args)
           v
+-----------------------------------------------+
|              VELOCITY RUNTIME                  |
|                                                 |
|  +-------------+   +----------------------+    |
|  | Worker Pool |   |  Async Scheduler     |    |
|  | (pre-warmed)|<--| (overlaps I/O with   |    |
|  +-------------+   |  LLM think-time)     |    |
|         |           +----------------------+    |
|         v                                       |
|  +-------------------------------------+        |
|  |   Binary Wire Protocol (codec)      |        |
|  +-------------------------------------+        |
|         |                                       |
|  +-------------------------------------+        |
|  |  Connection-Pooled Transport Layer  |        |
|  +-------------------------------------+        |
+----------+--------------------------------------+
           v
+----------------------+
|   Tool Executors       |  (mock DB, mock HTTP API, mock file I/O)
+----------------------+
```

---

## 5. Tech Stack

| Layer | Choice | Rationale |
|---|---|---|
| Core runtime language | **Rust** | Mature async ecosystem (tokio), memory safety without GC pauses, predictable latency |
| Async runtime | `tokio` | Industry standard, `io_uring` support available via `tokio-uring` if needed later |
| Wire protocol | Custom length-prefixed binary format (see §7) | Avoids JSON/Protobuf allocation overhead |
| Baseline A | LangGraph (Python) | Most common agent orchestration framework today |
| Baseline B | Raw MCP server (Python or TS reference impl) | Represents "no framework" floor |
| Benchmarking harness | Rust binary + Python driver scripts | Rust for precision timing, Python for baseline compatibility |
| Metrics/reporting | `hdrhistogram` (Rust) for latency percentiles; results exported to CSV/JSON | Standard for accurate p50/p95/p99 capture |
| Load generation | Custom async load generator (part of harness) | Full control over concurrency patterns |

**Explicitly rejected for v0:** C++ (learning curve cost vs. timeline), Protobuf (still allocates, still has overhead vs. hand-rolled binary), gRPC (adds HTTP/2 framing overhead we're trying to avoid measuring around).

---

## 6. Repository Structure

```
velocity/
├── AGENTS.md                     <- this file
├── README.md                     <- public-facing summary + results
├── Cargo.toml                    <- workspace root
├── crates/
│   ├── velocity-core/            <- worker pool, scheduler, runtime logic
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── worker_pool.rs
│   │   │   ├── scheduler.rs
│   │   │   └── transport.rs
│   │   └── Cargo.toml
│   ├── velocity-protocol/        <- binary codec (encode/decode)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── codec.rs
│   │   │   └── messages.rs
│   │   └── Cargo.toml
│   ├── velocity-tools/           <- mock tool executors
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── mock_db.rs
│   │   │   ├── mock_http.rs
│   │   │   └── mock_file.rs
│   │   └── Cargo.toml
│   └── velocity-bench/           <- benchmark harness + load generator
│       ├── src/
│       │   ├── main.rs
│       │   ├── task_definitions.rs
│       │   ├── load_gen.rs
│       │   └── report.rs
│       └── Cargo.toml
├── baselines/
│   ├── langgraph_baseline/       <- Python, Baseline A
│   │   ├── agent.py
│   │   ├── tools.py
│   │   └── requirements.txt
│   └── raw_mcp_baseline/         <- Python or TS, Baseline B
│       ├── server.py
│       └── requirements.txt
├── results/
│   ├── raw/                      <- raw benchmark output (CSV/JSON per run)
│   └── report.md                 <- final written analysis with graphs
└── scripts/
    ├── run_all_benchmarks.sh
    └── generate_graphs.py
```

---

## 7. Component Specifications

### 7.1 Binary Wire Protocol (`velocity-protocol`)

**Purpose:** eliminate JSON serialize/deserialize cost on every tool-call hop.

**Message format** (length-prefixed, fixed header):

```
+--------------+--------------+--------------+-------------------+
|  magic (2B)  | version (1B) | msg_type(1B) | payload_len (4B)  |  <- 8-byte header
+--------------+--------------+--------------+-------------------+
|                     payload (variable, binary)                  |
+-------------------------------------------------------------+
```

- `magic`: constant `0x56 0x4C` ("VL") for framing validation
- `msg_type`: `0x01 = ToolCallRequest`, `0x02 = ToolCallResponse`, `0x03 = Heartbeat`, `0x04 = Error`
- Payload encoding: hand-rolled struct packing (fixed-width fields where possible; variable-length strings prefixed with u32 length). No reflection, no schema negotiation at runtime — schemas are compile-time known for v0's fixed tool set.
- All integers little-endian.

**Required functions:**
```rust
pub fn encode_tool_call(req: &ToolCallRequest) -> Vec<u8>;
pub fn decode_tool_call(bytes: &[u8]) -> Result<ToolCallRequest, ProtocolError>;
pub fn encode_response(resp: &ToolCallResponse) -> Vec<u8>;
pub fn decode_response(bytes: &[u8]) -> Result<ToolCallResponse, ProtocolError>;
```

**Acceptance criteria:** encode+decode round trip for a representative payload must complete in under 5 microseconds on reference hardware (measured in a dedicated micro-benchmark using `criterion`).

---

### 7.2 Worker Pool (`velocity-core::worker_pool`)

**Purpose:** eliminate cold-start latency by keeping tool executors warm and ready.

**Requirements:**
- Fixed-size pool of pre-spawned worker tasks (configurable, default 16) per tool type.
- Workers are `tokio` tasks holding a persistent connection/handle to their tool executor (mock DB connection, mock HTTP client, open file handle) — never re-established per call.
- Pool exposes an acquire/release interface with a bounded wait queue (never unbounded spawning — that reintroduces cold-start risk under burst load).
- Idle workers send lightweight heartbeats (`msg_type = 0x03`) every N seconds to detect and replace dead workers proactively — never lazily on the request path.

**Required interface:**
```rust
pub struct WorkerPool {
    pub async fn acquire(&self, tool_name: &str) -> Result<WorkerHandle, PoolError>;
    pub fn release(&self, handle: WorkerHandle);
    pub fn stats(&self) -> PoolStats; // active, idle, queued counts
}
```

**Acceptance criteria:** acquiring a warm worker under normal load must complete in under 10 microseconds (no allocation, no syscalls beyond a channel recv).

---

### 7.3 Async Scheduler (`velocity-core::scheduler`)

**Purpose:** overlap LLM "thinking" latency with tool I/O wherever the agent's task graph allows it (e.g., speculatively pre-fetch a likely-next tool's connection while the LLM is still generating the current tool call's arguments).

**Requirements:**
- Scheduler accepts a stream of tool-call intents and executes independent calls concurrently (no artificial serialization when calls have no data dependency).
- Supports a "speculative warm" mode: given a hint of the next likely tool (from a static task graph in the benchmark, not from real LLM introspection — that's out of scope for v0), pre-acquire that worker before it's formally requested.
- Must expose clean tracing/instrumentation hooks (timestamps at: request received, worker acquired, tool executed, response returned) for the benchmark harness to consume.

**Acceptance criteria:** demonstrable reduction in total task completion time (not just individual call latency) versus fully serial execution, on a task with at least 2 independent tool calls.

---

### 7.4 Mock Tool Executors (`velocity-tools`)

Three tools, deliberately simple, deliberately realistic in latency shape:

1. **mock_db** — simulates a DB query with a configurable artificial delay (default 5–15ms jittered) to mimic real query latency; returns a small fixed-schema record.
2. **mock_http** — simulates an external API call with configurable delay (default 20–50ms jittered) representing a third-party pricing/inventory API.
3. **mock_file** — simulates a file read/write with minimal delay (default 1–3ms), representing local I/O.

All three must be implemented identically (same delay distributions) across the Velocity runtime, Baseline A, and Baseline B, so the benchmark isolates *orchestration overhead*, not tool-implementation differences.

---

### 7.5 Benchmark Harness (`velocity-bench`)

**This is the most important deliverable in the entire MVP.** It IS the product's evidence.

**Task definition (must be identical across all 3 contenders):**

```
Task: "process_order"
  1. call mock_db("lookup_account", account_id)
  2. call mock_db("check_inventory", sku)         [independent of step 1 — can overlap]
  3. call mock_http("get_pricing", sku)            [depends on step 2]
  4. call mock_db("write_order_record", ...)       [depends on steps 1 & 3]
  5. call mock_file("write_confirmation_log", ...) [depends on step 4]
```

This task has both dependent and independent branches — a realistic shape, not a trivial linear chain — so the scheduler's overlap capability is actually exercised and visible in the numbers.

**Required measurements, per contender, per run:**
- Per-call latency: p50, p95, p99, max
- Total task completion time: p50, p95, p99
- Cold-start penalty (first call vs. steady-state calls)
- Throughput and latency degradation under concurrent load: 10, 100, 1,000 simultaneous task executions

**Output format:** raw results as CSV/JSON in `results/raw/`, one file per (contender × concurrency level) combination. Final human-readable report in `results/report.md` with generated graphs (bar charts of p50/p95/p99 per contender, line chart of latency vs. concurrency).

**Acceptance criteria for the MVP as a whole:** the report must present real numbers for all three contenders at all four concurrency levels, with methodology described in enough detail that an external engineer could reproduce every result from the repo alone. If Velocity does NOT beat the baselines in some dimension, that must be reported honestly, not omitted.

---

## 8. Baselines — Build Requirements

### 8.1 Baseline A — LangGraph
- Implement the identical `process_order` task graph using LangGraph's standard tool-calling with the three mock tools re-implemented in Python with matching delay distributions.
- Use standard LangGraph patterns — no artificial handicapping, no artificial optimization. This must represent a competent, idiomatic LangGraph implementation.

### 8.2 Baseline B — Raw MCP
- Implement the same task using a bare MCP server (stdio or SSE transport, whichever is more standard at implementation time) with no orchestration framework layered on top — just direct tool-call requests issued in the same dependency order as the task graph.

Both baselines must be instrumented with the same timestamp hooks (request sent, response received) so measurement methodology is apples-to-apples.

---

## 9. Milestones & Timeline

| Week | Deliverables | Exit criteria |
|---|---|---|
| **Week 1** | `velocity-protocol` codec complete + micro-benchmarked; `velocity-core` worker pool complete; basic scheduler skeleton | Codec round-trips under 5μs; worker acquire under 10μs; unit tests passing |
| **Week 2** | Mock tools implemented (all 3, matching across runtime + both baselines); full `process_order` task running end-to-end on all 3 contenders; single-request latency numbers collected | All 3 contenders produce correct output for the task; first raw latency numbers exist |
| **Week 3** | Load testing at 10/100/1000 concurrency; report.md written with graphs; repo cleaned, README written, published | Reproducible benchmark run documented step-by-step; honest report published publicly |

---

## 10. Coding Standards

- **Rust code:** `cargo fmt` and `cargo clippy --all-targets -- -D warnings` must pass clean before any commit is considered done. No `unwrap()` in runtime hot paths — use proper `Result` propagation; `unwrap()` is acceptable only in test code or the benchmark harness's non-hot-path setup code.
- **No premature abstraction.** Do not build a generic "plugin system" for tools — three hardcoded tool implementations are correct for v0. Generalize only after the hypothesis is validated.
- **Every public function in `velocity-core` and `velocity-protocol` requires a doc comment** explaining latency-relevant behavior (e.g., "this function performs zero heap allocations on the fast path").
- **Instrumentation is not optional.** Every hop in the runtime that could plausibly show up as latency in the benchmark must emit a timestamp via the tracing hooks — retrofitting instrumentation after the fact produces untrustworthy numbers.
- **Commit messages** should reference which section of this AGENTS.md they implement (e.g., `feat(protocol): implement binary codec per §7.1`).

---

## 11. Testing Requirements

- Unit tests for codec encode/decode round trips, including malformed-input edge cases (truncated payload, bad magic bytes, oversized payload).
- Integration test running the full `process_order` task against the Velocity runtime with mock tools, asserting correct output ordering and dependency resolution.
- Load test harness must be re-runnable via a single script (`scripts/run_all_benchmarks.sh`) with no manual steps, to guarantee reproducibility for anyone cloning the repo.

---

## 12. Success Metrics (how we know the MVP is done)

1. A public GitHub repo exists with all code above, buildable via `cargo build --release` and `./scripts/run_all_benchmarks.sh`.
2. `results/report.md` contains real, reproducible latency numbers for all 3 contenders across all 4 concurrency levels.
3. The report states plainly whether the 5–10x hypothesis held, partially held, or did not hold — with numbers, not adjectives.
4. An external engineer unfamiliar with the project can clone the repo and reproduce the headline number within a reasonable margin, using only the README.

---

## 13. Known Risks / Explicit Non-Handling for v0

- **Real LLM think-time overlap is simulated, not real.** v0 uses a static task graph with known dependencies rather than actual live LLM tool-call decisions. This is an accepted simplification — clearly disclosed in the report — because wiring a live LLM into the benchmark loop would introduce non-deterministic latency that pollutes the very numbers we're trying to isolate.
- **Single-machine benchmarking only.** No distributed/multi-region testing in v0; this is a controlled, single-host comparison. Disclosed as a scope limitation, not hidden.
- **Mock tools, not real integrations.** Real-world tools (actual DBs, actual third-party APIs) have their own latency variance that would need separate characterization — explicitly deferred to a post-MVP phase.

---

## 14. What Comes After v0 (not to be built now, but noted for context)

- Real LLM-in-the-loop benchmarking (replace static task graph with live model decisions)
- SDK/client libraries (Python, TS) so real agent frameworks can adopt the runtime without rewriting in Rust
- C++ implementation for teams requiring it (HFT-adjacent shops)
- Multi-tool-type generalization beyond the 3 hardcoded mocks
- Distributed/multi-region deployment support

None of the above is in scope until the MVP hypothesis (§3) is proven with real numbers.

---

*End of spec. Any ambiguity encountered during implementation should be resolved in favor of the narrowest interpretation that still lets the benchmark in §7.5 run honestly — when in doubt, cut scope, not rigor.*
