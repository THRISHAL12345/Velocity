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

## Results

See [`results/report.md`](results/report.md) for the full benchmark analysis.

## License

MIT
