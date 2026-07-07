#!/usr/bin/env bash
# =============================================================================
# Velocity Benchmark Suite — Run All Contenders
# =============================================================================
#
# Runs the Velocity runtime, Python asyncio baseline, and raw MCP baseline
# benchmarks at all concurrency levels (1, 10, 100, 1000).
#
# Usage: ./scripts/run_all_benchmarks.sh
#
# Requirements:
#   - Rust toolchain (cargo)
#   - Python 3.8+
#
# Output: results/raw/ (CSV + JSON per contender per concurrency level)
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RESULTS_DIR="$PROJECT_ROOT/results/raw"

echo ""
echo "================================================================"
echo "  VELOCITY BENCHMARK SUITE"
echo "================================================================"
echo ""
echo "Project root: $PROJECT_ROOT"
echo "Results dir:  $RESULTS_DIR"
echo ""

# Ensure results directory exists
mkdir -p "$RESULTS_DIR"

# ─── Step 1: Build Velocity runtime in release mode ─────────────────────────

echo "📦 Building Velocity runtime (release mode)..."
cd "$PROJECT_ROOT"
cargo build --release -p velocity-bench 2>&1
echo "   ✓ Build complete"
echo ""

# ─── Step 2: Run Velocity benchmark ─────────────────────────────────────────

echo "🚀 Running Velocity benchmark..."
cargo run --release -p velocity-bench -- \
    --output-dir "$RESULTS_DIR" \
    --concurrency "1,10,100,1000" \
    --pool-size 64 \
    --warmup 5 \
    --iterations 50
echo ""

# ─── Step 3: Run LangGraph baseline ─────────────────────────────────────────

echo "🕸️ Running LangGraph baseline..."
cd "$PROJECT_ROOT/baselines/langgraph_baseline"
python3 agent.py
echo ""

# ─── Step 4: Run raw MCP baseline ───────────────────────────────────────────

echo "📡 Running raw MCP baseline..."
cd "$PROJECT_ROOT/baselines/raw_mcp_baseline"
python3 server.py
echo ""

# ─── Step 5: Generate graphs ────────────────────────────────────────────────

echo "📊 Generating comparison graphs..."
cd "$PROJECT_ROOT"
python3 scripts/generate_graphs.py
echo ""

# ─── Done ────────────────────────────────────────────────────────────────────

echo "================================================================"
echo "  ALL BENCHMARKS COMPLETE"
echo "================================================================"
echo ""
echo "Results written to: $RESULTS_DIR"
echo "Report: results/report.md"
echo ""
