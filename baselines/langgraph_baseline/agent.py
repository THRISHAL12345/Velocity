"""LangGraph baseline for the Velocity benchmark.

Implements the identical `process_order` task graph using standard Python
asyncio with the same dependency structure:
  Step 1: mock_db("lookup_account", account_id)     [no deps]
  Step 2: mock_db("check_inventory", sku)            [no deps, independent of 1]
  Step 3: mock_http("get_pricing", sku)              [depends on 2]
  Step 4: mock_db("write_order_record", ...)         [depends on 1 & 3]
  Step 5: mock_file("write_confirmation_log", ...)   [depends on 4]

This baseline uses asyncio.gather for independent steps and sequential
awaits for dependent steps, representing a competent Python async
implementation without LangGraph framework overhead.

NOTE: We implement this as a pure asyncio baseline rather than requiring
LangGraph installation, since LangGraph adds framework overhead that is
separate from the orchestration overhead we're measuring. This gives
Python the fairest possible comparison — just asyncio coordination.
"""

import asyncio
import json
import sys
import time
import statistics
from pathlib import Path

from tools import mock_db, mock_http, mock_file


async def process_order(account_id: str, sku: str) -> dict:
    """Execute the process_order task graph with proper dependency handling.

    Steps 1 and 2 are independent and run concurrently via asyncio.gather.
    Steps 3-5 are sequential based on their dependencies.
    """
    start = time.perf_counter_ns()
    step_times = {}

    # Steps 1 & 2: independent — run concurrently
    s1_start = time.perf_counter_ns()
    s2_start = time.perf_counter_ns()

    result_1, result_2 = await asyncio.gather(
        mock_db("lookup_account", account_id=account_id),
        mock_db("check_inventory", sku=sku),
    )

    s1_end = time.perf_counter_ns()
    s2_end = time.perf_counter_ns()
    step_times["step_1"] = (s1_end - s1_start) / 1000  # ns -> us
    step_times["step_2"] = (s2_end - s2_start) / 1000

    # Step 3: depends on step 2
    s3_start = time.perf_counter_ns()
    result_3 = await mock_http("get_pricing", sku=sku)
    s3_end = time.perf_counter_ns()
    step_times["step_3"] = (s3_end - s3_start) / 1000

    # Step 4: depends on steps 1 & 3
    s4_start = time.perf_counter_ns()
    result_4 = await mock_db(
        "write_order_record", account_id=account_id, sku=sku
    )
    s4_end = time.perf_counter_ns()
    step_times["step_4"] = (s4_end - s4_start) / 1000

    # Step 5: depends on step 4
    s5_start = time.perf_counter_ns()
    result_5 = await mock_file(
        "write_confirmation_log", order_id=result_4.get("order_id", "UNKNOWN")
    )
    s5_end = time.perf_counter_ns()
    step_times["step_5"] = (s5_end - s5_start) / 1000

    total_us = (time.perf_counter_ns() - start) / 1000

    return {
        "total_us": total_us,
        "step_times": step_times,
        "results": {
            "step_1": result_1,
            "step_2": result_2,
            "step_3": result_3,
            "step_4": result_4,
            "step_5": result_5,
        },
    }


async def run_benchmark(concurrency: int, iterations: int, warmup: int) -> dict:
    """Run the benchmark at a given concurrency level."""

    # Warm-up
    for _ in range(warmup):
        await process_order("WARMUP-ACC", "WARMUP-SKU")

    all_task_times = []
    all_step_times = {f"step_{i}": [] for i in range(1, 6)}
    cold_start_us = 0

    for iteration in range(iterations):
        start = time.perf_counter_ns()

        # Launch concurrent tasks
        tasks = [
            process_order(f"ACC-{i:05d}", f"SKU-{i:05d}")
            for i in range(concurrency)
        ]
        results = await asyncio.gather(*tasks)

        batch_us = (time.perf_counter_ns() - start) / 1000

        if iteration == 0:
            cold_start_us = batch_us

        for result in results:
            all_task_times.append(result["total_us"])
            for step_id, step_time in result["step_times"].items():
                all_step_times[step_id].append(step_time)

    # Compute percentiles
    all_task_times.sort()
    n = len(all_task_times)

    def percentile(data, p):
        if not data:
            return 0
        sorted_data = sorted(data)
        k = (len(sorted_data) - 1) * (p / 100.0)
        f = int(k)
        c = f + 1 if f + 1 < len(sorted_data) else f
        return sorted_data[f] + (k - f) * (sorted_data[c] - sorted_data[f])

    task_stats = {
        "p50_us": percentile(all_task_times, 50),
        "p95_us": percentile(all_task_times, 95),
        "p99_us": percentile(all_task_times, 99),
        "max_us": max(all_task_times) if all_task_times else 0,
        "min_us": min(all_task_times) if all_task_times else 0,
        "mean_us": statistics.mean(all_task_times) if all_task_times else 0,
        "count": len(all_task_times),
    }

    step_stats = {}
    for step_id, times in all_step_times.items():
        step_stats[step_id] = {
            "p50_us": percentile(times, 50),
            "p95_us": percentile(times, 95),
            "p99_us": percentile(times, 99),
            "max_us": max(times) if times else 0,
            "min_us": min(times) if times else 0,
            "mean_us": statistics.mean(times) if times else 0,
            "count": len(times),
        }

    # Compute steady-state average (excluding first batch)
    steady_times = all_task_times[concurrency:] if len(all_task_times) > concurrency else all_task_times
    steady_state_avg = statistics.mean(steady_times) if steady_times else 0

    return {
        "contender": "python_asyncio",
        "concurrency": concurrency,
        "task_stats": task_stats,
        "step_stats": step_stats,
        "cold_start_us": cold_start_us,
        "steady_state_avg_us": steady_state_avg,
    }


async def main():
    """Run benchmarks at all concurrency levels and output results."""
    concurrency_levels = [1, 10, 100, 1000]
    warmup = 5
    iterations = 50
    output_dir = Path("../../results/raw")
    output_dir.mkdir(parents=True, exist_ok=True)

    print("\n🐍 Python Asyncio Baseline Benchmark")
    print(f"   Warm-up: {warmup} iterations")
    print(f"   Measured: {iterations} iterations")
    print(f"   Concurrency levels: {concurrency_levels}\n")

    all_reports = []

    for concurrency in concurrency_levels:
        print(f"⏱  Running benchmark at concurrency={concurrency}...")

        report = await run_benchmark(concurrency, iterations, warmup)

        # Write results
        filename = f"python_asyncio_{concurrency}"
        with open(output_dir / f"{filename}.json", "w") as f:
            json.dump(report, f, indent=2)

        print(
            f"   ✓ concurrency={concurrency}: "
            f"p50={report['task_stats']['p50_us']:.0f}μs  "
            f"p95={report['task_stats']['p95_us']:.0f}μs  "
            f"p99={report['task_stats']['p99_us']:.0f}μs"
        )

        all_reports.append(report)

    # Print summary
    print(f"\n{'=' * 80}")
    print("  PYTHON ASYNCIO BASELINE RESULTS")
    print(f"{'=' * 80}\n")

    for report in all_reports:
        stats = report["task_stats"]
        print(f"Concurrency: {report['concurrency']}")
        print(f"  Task Total:  p50={stats['p50_us']:>8.0f}μs  "
              f"p95={stats['p95_us']:>8.0f}μs  "
              f"p99={stats['p99_us']:>8.0f}μs  "
              f"max={stats['max_us']:>8.0f}μs")
        print(f"  Cold Start:  {report['cold_start_us']:>8.0f}μs  "
              f"|  Steady-State Avg: {report['steady_state_avg_us']:>8.0f}μs")
        print()

    print(f"📊 Raw results written to: {output_dir}")
    print("✅ Benchmark complete.\n")


if __name__ == "__main__":
    asyncio.run(main())
