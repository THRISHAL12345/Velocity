# Velocity — Low-Latency Agent Tool-Call Execution Runtime

Velocity is a purpose-built execution runtime for AI agent tool-calling that targets **5–10x lower tool-call round-trip latency** versus standard Python-based orchestration (LangGraph, raw MCP-over-stdio).

## How It Works

Velocity eliminates three specific sources of overhead:

1. **JSON serialization cost** → replaced with a custom binary wire protocol
2. **Cold-start latency** → eliminated via pre-warmed worker pools
3. **Serial scheduling** → replaced with an async scheduler that overlaps LLM think-time with tool I/O

## Project Status

🚧 **v0 / MVP** — Building the runtime core + reproducible benchmark to prove/disprove the central latency claim.

## Repository Structure

```
velocity/
├── crates/
│   ├── velocity-protocol/    # Binary wire protocol codec
│   ├── velocity-core/        # Worker pool, scheduler, transport
│   ├── velocity-tools/       # Mock tool executors
│   └── velocity-bench/       # Benchmark harness + load generator
├── baselines/
│   ├── langgraph_baseline/   # Python — Baseline A
│   └── raw_mcp_baseline/     # Python — Baseline B
├── results/                  # Benchmark output + report
└── scripts/                  # Automation scripts
```

## Building

```bash
cargo build --release
```

## Running Benchmarks

```bash
./scripts/run_all_benchmarks.sh
```

## Results & Key Findings

v0 proves the runtime beats LangGraph by up to **2.2x at p99 under load**, successfully eliminating framework scheduling overhead and JSON serialization costs. More importantly, our rigorous empirical benchmark surfaces a critical systems finding: **bounded worker pools lose to unbounded coroutines at extreme concurrency**, and binary protocol savings (<5μs round-trip) are largely invisible when simulated tool I/O dominates (~90ms cumulative I/O time per task). Framing these trade-offs honestly is central to Velocity's design philosophy—v1 directly targets both constraints.

For full empirical data, generated charts, and methodology, see [`results/report.md`](results/report.md).

## Known Limitations / v1 Roadmap

To evolve from a proven core runtime into a production-ready agent execution engine, **v1** addresses the three primary architectural constraints identified in v0:

1. **Dynamic Worker Scaling (Variable Pool Sizing)**: Replacing fixed MPSC channel capacities with adaptive, elastic worker pools that scale up under sudden concurrency bursts to eliminate queue contention while preventing system resource exhaustion.
2. **Low-Latency Tool Profile (<100μs Tools)**: Introducing sub-millisecond benchmark tasks (e.g., in-memory vector search, local state lookups, or HFT calculation engines) where serialization and dispatch latency represent a dominant percentage of round-trip time.
3. **Fair Concurrency Caps on Baselines**: Enforcing realistic connection pooling and file-descriptor limits on raw asyncio/MCP baselines to ensure apples-to-apples comparisons under extreme concurrent load.

## License

MIT
