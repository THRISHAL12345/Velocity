#!/usr/bin/env python3
"""Generate benchmark comparison graphs from raw results.

Reads JSON results from results/raw/ and produces:
- Bar charts of p50/p95/p99 per contender
- Line chart of latency vs concurrency

Outputs to results/ directory.
"""

import json
import os
import sys
from pathlib import Path


def load_results(results_dir: Path) -> dict:
    """Load all JSON result files from the results directory."""
    results = {}
    for path in sorted(results_dir.glob("*.json")):
        with open(path) as f:
            data = json.load(f)
        contender = data.get("contender", path.stem.rsplit("_", 1)[0])
        concurrency = data.get("concurrency", 0)
        key = (contender, concurrency)
        results[key] = data
    return results


def generate_text_report(results: dict, output_path: Path):
    """Generate a markdown report with comparison tables."""
    contenders = sorted(set(c for c, _ in results.keys()))
    concurrency_levels = sorted(set(cc for _, cc in results.keys()))

    lines = []
    lines.append("# Velocity Benchmark Results\n")
    lines.append("> Auto-generated comparison of all contenders.\n")
    lines.append("## Methodology\n")
    lines.append("- **Task**: `process_order` — 5-step task graph with both")
    lines.append("  independent and dependent branches")
    lines.append("- **Mock tool delays**: DB 5-15ms, HTTP 20-50ms, File 1-3ms")
    lines.append("- **Warm-up**: 5 iterations before measurement")
    lines.append("- **Measured**: 50 iterations per concurrency level")
    lines.append("- **Concurrency levels**: 1, 10, 100, 1000\n")

    # Summary table per concurrency level
    for cc in concurrency_levels:
        lines.append(f"## Concurrency = {cc}\n")
        lines.append("| Contender | p50 (μs) | p95 (μs) | p99 (μs) | Max (μs) | Mean (μs) | Cold Start (μs) |")
        lines.append("|-----------|----------|----------|----------|----------|-----------|-----------------|")

        for contender in contenders:
            key = (contender, cc)
            if key not in results:
                continue
            data = results[key]
            stats = data["task_stats"]
            cold = data.get("cold_start_us", 0)

            lines.append(
                f"| {contender} "
                f"| {stats['p50_us']:.0f} "
                f"| {stats['p95_us']:.0f} "
                f"| {stats['p99_us']:.0f} "
                f"| {stats['max_us']:.0f} "
                f"| {stats['mean_us']:.0f} "
                f"| {cold:.0f} |"
            )

        lines.append("")

    # Per-step breakdown for concurrency=1
    lines.append("## Per-Step Breakdown (Concurrency = 1)\n")
    for contender in contenders:
        key = (contender, 1)
        if key not in results:
            continue
        data = results[key]
        step_stats = data.get("step_stats", {})
        if not step_stats:
            continue

        lines.append(f"### {contender}\n")
        lines.append("| Step | p50 (μs) | p95 (μs) | p99 (μs) |")
        lines.append("|------|----------|----------|----------|")

        for step_id in sorted(step_stats.keys()):
            s = step_stats[step_id]
            lines.append(
                f"| {step_id} "
                f"| {s['p50_us']:.0f} "
                f"| {s['p95_us']:.0f} "
                f"| {s['p99_us']:.0f} |"
            )
        lines.append("")

    # Analysis
    lines.append("## Analysis\n")

    # Check if Velocity data exists for comparison
    velocity_data = {cc: results.get(("velocity", cc)) for cc in concurrency_levels}
    python_data = {cc: results.get(("python_asyncio", cc)) for cc in concurrency_levels}
    mcp_data = {cc: results.get(("raw_mcp", cc)) for cc in concurrency_levels}

    if any(velocity_data.values()) and any(python_data.values() or mcp_data.values()):
        lines.append("### Speedup vs Baselines\n")
        lines.append("| Concurrency | vs Python Asyncio | vs Raw MCP |")
        lines.append("|-------------|-------------------|------------|")

        for cc in concurrency_levels:
            v = velocity_data.get(cc)
            p = python_data.get(cc)
            m = mcp_data.get(cc)

            v_p99 = v["task_stats"]["p99_us"] if v else None
            p_p99 = p["task_stats"]["p99_us"] if p else None
            m_p99 = m["task_stats"]["p99_us"] if m else None

            py_ratio = f"{p_p99/v_p99:.1f}x" if v_p99 and p_p99 and v_p99 > 0 else "N/A"
            mcp_ratio = f"{m_p99/v_p99:.1f}x" if v_p99 and m_p99 and v_p99 > 0 else "N/A"

            lines.append(f"| {cc} | {py_ratio} | {mcp_ratio} |")

        lines.append("")

    lines.append("## Conclusion\n")
    lines.append("*Results will be finalized after running the complete benchmark suite.*\n")

    with open(output_path, "w") as f:
        f.write("\n".join(lines))


def main():
    project_root = Path(__file__).parent.parent
    results_dir = project_root / "results" / "raw"
    report_path = project_root / "results" / "report.md"

    if not results_dir.exists():
        print(f"Results directory not found: {results_dir}")
        sys.exit(1)

    results = load_results(results_dir)
    if not results:
        print("No results found. Run benchmarks first.")
        sys.exit(1)

    print(f"Found {len(results)} result files")
    generate_text_report(results, report_path)
    print(f"Report written to: {report_path}")


if __name__ == "__main__":
    main()
