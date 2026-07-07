#!/usr/bin/env bash
# =============================================================================
# Velocity Benchmark Suite — Run All Contenders & Experiment Profiles
# =============================================================================
#
# Runs the Velocity runtime, LangGraph baseline, and raw MCP baseline
# across multiple experiment configurations:
#   1. Standard process_order profile (concurrency 1..1000, pool_size 64)
#   2. Pool Size Sweep (process_order at conc 1000, pool sizes 64..4096)
#   3. HFT Tick Low-Latency Profile (concurrency 1..1000, pool_size 64)
#
# Output: results/raw/ (CSV + JSON per run)
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RESULTS_DIR="$PROJECT_ROOT/results/raw"

PYTHON_CMD=$(command -v python3 2>/dev/null || command -v python)

echo ""
echo "================================================================"
echo "  VELOCITY BENCHMARK SUITE — SYSTEM EXPERIMENTS"
echo "================================================================"
echo ""
echo "Project root: $PROJECT_ROOT"
echo "Results dir:  $RESULTS_DIR"
echo "Python cmd:   $PYTHON_CMD"
echo ""

# Ensure results directory exists and clean old results
mkdir -p "$RESULTS_DIR"
rm -f "$RESULTS_DIR"/*.json "$RESULTS_DIR"/*.csv 2>/dev/null || true

# ─── Step 1: Build Velocity runtime in release mode ─────────────────────────

echo "📦 Building Velocity runtime (release mode)..."
cd "$PROJECT_ROOT"
cargo build --release -p velocity-bench 2>&1
echo "   ✓ Build complete"
echo ""

# ─── Step 2: Standard process_order Profile ──────────────────────────────────

echo "🚀 [1/4] Running Velocity standard benchmark (process_order)..."
cargo run --release -p velocity-bench -- \
    --output-dir "$RESULTS_DIR" \
    --concurrency "1,10,100,1000" \
    --pool-size 64 \
    --warmup 5 \
    --iterations 50 \
    --profile process_order
echo ""

echo "🕸️ [1/4] Running LangGraph standard baseline..."
cd "$PROJECT_ROOT/baselines/langgraph_baseline"
$PYTHON_CMD agent.py --concurrency "1,10,100,1000" --profile process_order
echo ""

echo "📡 [1/4] Running raw MCP standard baseline..."
cd "$PROJECT_ROOT/baselines/raw_mcp_baseline"
$PYTHON_CMD server.py --concurrency "1,10,100,1000" --profile process_order
echo ""

# ─── Step 3: Pool Size Sweep (concurrency=1000) ──────────────────────────────

echo "🚀 [2/4] Running Velocity pool size sweep (concurrency=1000)..."
cd "$PROJECT_ROOT"
cargo run --release -p velocity-bench -- \
    --output-dir "$RESULTS_DIR" \
    --concurrency "1000" \
    --sweep-pool-sizes "64,256,1024,4096" \
    --warmup 5 \
    --iterations 50 \
    --profile process_order
echo ""

echo "📡 [2/4] Running fair-capped raw MCP pool size sweep (concurrency=1000)..."
cd "$PROJECT_ROOT/baselines/raw_mcp_baseline_capped"
for p in 64 256 1024 4096; do
    echo "   Testing capped MCP with semaphore pool_size=$p..."
    $PYTHON_CMD server.py --concurrency "1000" --pool-size "$p" --profile process_order
done
echo ""

# ─── Step 4: HFT Tick Low-Latency Profile ────────────────────────────────────

echo "🚀 [3/4] Running Velocity HFT low-latency profile..."
cd "$PROJECT_ROOT"
cargo run --release -p velocity-bench -- \
    --output-dir "$RESULTS_DIR" \
    --concurrency "1,10,100,1000" \
    --pool-size 64 \
    --warmup 5 \
    --iterations 50 \
    --profile hft_tick
echo ""

echo "🕸️ [3/4] Running LangGraph HFT low-latency profile..."
cd "$PROJECT_ROOT/baselines/langgraph_baseline"
$PYTHON_CMD agent.py --concurrency "1,10,100,1000" --profile hft_tick
echo ""

echo "📡 [3/4] Running raw MCP HFT low-latency profile..."
cd "$PROJECT_ROOT/baselines/raw_mcp_baseline"
$PYTHON_CMD server.py --concurrency "1,10,100,1000" --profile hft_tick
echo ""

# ─── Step 5: Generate graphs & report ────────────────────────────────────────

echo "📊 [4/4] Generating comparison graphs and analysis report..."
cd "$PROJECT_ROOT"
$PYTHON_CMD scripts/generate_graphs.py
echo ""

# ─── Done ────────────────────────────────────────────────────────────────────

echo "================================================================"
echo "  ALL BENCHMARKS & EXPERIMENTS COMPLETE"
echo "================================================================"
echo ""
echo "Results written to: $RESULTS_DIR"
echo "Report: results/report.md"
echo ""
