"""LangGraph baseline for the Velocity benchmark.

Implements the identical `process_order` task graph using an idiomatic
LangGraph StateGraph per Section 8.1 of AGENTS.md:
  Step 1: mock_db("lookup_account", account_id)     [no deps]
  Step 2: mock_db("check_inventory", sku)            [no deps, independent of 1]
  Step 3: mock_http("get_pricing", sku)              [depends on 2]
  Step 4: mock_db("write_order_record", ...)         [depends on 1 & 3]
  Step 5: mock_file("write_confirmation_log", ...)   [depends on 4]

This baseline uses LangGraph's compiled StateGraph engine, representing
a competent, idiomatic LangGraph implementation without artificial handicapping.
"""

import asyncio
import json
import sys
import time
import statistics
from pathlib import Path
from typing import TypedDict, Annotated, Any, Dict

from langgraph.graph import StateGraph, START, END
from tools import mock_db, mock_http, mock_file


def merge_dicts(left: dict, right: dict) -> dict:
    """Reducer to cleanly merge state dictionaries from concurrent nodes."""
    return {**left, **right}


class OrderState(TypedDict):
    account_id: str
    sku: str
    step_times: Annotated[dict, merge_dicts]
    results: Annotated[dict, merge_dicts]


async def step_1_node(state: OrderState) -> dict:
    t0 = time.perf_counter_ns()
    res = await mock_db("lookup_account", account_id=state["account_id"])
    dt = (time.perf_counter_ns() - t0) / 1000
    return {"step_times": {"step_1": dt}, "results": {"step_1": res}}


async def step_2_node(state: OrderState) -> dict:
    t0 = time.perf_counter_ns()
    res = await mock_db("check_inventory", sku=state["sku"])
    dt = (time.perf_counter_ns() - t0) / 1000
    return {"step_times": {"step_2": dt}, "results": {"step_2": res}}


async def step_3_node(state: OrderState) -> dict:
    t0 = time.perf_counter_ns()
    res = await mock_http("get_pricing", sku=state["sku"])
    dt = (time.perf_counter_ns() - t0) / 1000
    return {"step_times": {"step_3": dt}, "results": {"step_3": res}}


async def step_4_node(state: OrderState) -> dict:
    t0 = time.perf_counter_ns()
    res = await mock_db("write_order_record", account_id=state["account_id"], sku=state["sku"])
    dt = (time.perf_counter_ns() - t0) / 1000
    return {"step_times": {"step_4": dt}, "results": {"step_4": res}}


async def step_5_node(state: OrderState) -> dict:
    t0 = time.perf_counter_ns()
    order_id = state["results"].get("step_4", {}).get("order_id", "UNKNOWN")
    res = await mock_file("write_confirmation_log", order_id=order_id)
    dt = (time.perf_counter_ns() - t0) / 1000
    return {"step_times": {"step_5": dt}, "results": {"step_5": res}}


# Compile the LangGraph DAG
builder = StateGraph(OrderState)
builder.add_node("step_1", step_1_node)
builder.add_node("step_2", step_2_node)
builder.add_node("step_3", step_3_node)
builder.add_node("step_4", step_4_node)
builder.add_node("step_5", step_5_node)

# Steps 1 & 2 launch concurrently from START
builder.add_edge(START, "step_1")
builder.add_edge(START, "step_2")

# Step 3 depends on step 2
builder.add_edge("step_2", "step_3")

# Step 4 depends on steps 1 & 3 (synchronization barrier)
builder.add_edge("step_1", "step_4")
builder.add_edge("step_3", "step_4")

# Step 5 depends on step 4
builder.add_edge("step_4", "step_5")
builder.add_edge("step_5", END)

graph = builder.compile()


async def process_order(account_id: str, sku: str) -> dict:
    """Execute the process_order task graph via LangGraph."""
    start = time.perf_counter_ns()
    initial_state: OrderState = {
        "account_id": account_id,
        "sku": sku,
        "step_times": {},
        "results": {},
    }
    final_state = await graph.ainvoke(initial_state)
    total_us = (time.perf_counter_ns() - start) / 1000

    return {
        "total_us": total_us,
        "step_times": final_state["step_times"],
        "results": final_state["results"],
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
        "contender": "langgraph",
        "concurrency": concurrency,
        "task_stats": task_stats,
        "step_stats": step_stats,
        "cold_start_us": cold_start_us,
        "steady_state_avg_us": steady_state_avg,
    }


async def main():
    """Run benchmarks at all concurrency levels and output results."""
    if hasattr(sys.stdout, 'reconfigure'):
        sys.stdout.reconfigure(encoding='utf-8')
    concurrency_levels = [1, 10, 100, 1000]
    warmup = 5
    iterations = 50
    output_dir = Path(__file__).parent.parent.parent / "results" / "raw"
    output_dir.mkdir(parents=True, exist_ok=True)

    print("\n🕸️ LangGraph Baseline Benchmark", flush=True)
    print(f"   Warm-up: {warmup} iterations", flush=True)
    print(f"   Measured: {iterations} iterations", flush=True)
    print(f"   Concurrency levels: {concurrency_levels}\n", flush=True)

    all_reports = []

    for concurrency in concurrency_levels:
        print(f"⏱  Running benchmark at concurrency={concurrency}...", flush=True)

        report = await run_benchmark(concurrency, iterations, warmup)

        # Write results
        filename = f"langgraph_{concurrency}"
        with open(output_dir / f"{filename}.json", "w", encoding="utf-8") as f:
            json.dump(report, f, indent=2)
        with open(output_dir / f"{filename}.csv", "w", encoding="utf-8") as f:
            f.write("metric,p50_us,p95_us,p99_us,max_us,min_us,mean_us,count\n")
            ts = report["task_stats"]
            f.write(f"task_total,{ts['p50_us']:.0f},{ts['p95_us']:.0f},{ts['p99_us']:.0f},{ts['max_us']:.0f},{ts['min_us']:.0f},{ts['mean_us']:.2f},{ts['count']}\n")
            for step_id in sorted(report["step_stats"].keys()):
                ss = report["step_stats"][step_id]
                f.write(f"{step_id},{ss['p50_us']:.0f},{ss['p95_us']:.0f},{ss['p99_us']:.0f},{ss['max_us']:.0f},{ss['min_us']:.0f},{ss['mean_us']:.2f},{ss['count']}\n")

        print(
            f"   ✓ concurrency={concurrency}: "
            f"p50={report['task_stats']['p50_us']:.0f}μs  "
            f"p95={report['task_stats']['p95_us']:.0f}μs  "
            f"p99={report['task_stats']['p99_us']:.0f}μs", flush=True
        )

        all_reports.append(report)

    # Print summary
    print(f"\n{'=' * 80}", flush=True)
    print("  LANGGRAPH BASELINE RESULTS", flush=True)
    print(f"{'=' * 80}\n", flush=True)

    for report in all_reports:
        stats = report["task_stats"]
        print(f"Concurrency: {report['concurrency']}", flush=True)
        print(f"  Task Total:  p50={stats['p50_us']:>8.0f}μs  "
              f"p95={stats['p95_us']:>8.0f}μs  "
              f"p99={stats['p99_us']:>8.0f}μs  "
              f"max={stats['max_us']:>8.0f}μs", flush=True)
        print(f"  Cold Start:  {report['cold_start_us']:>8.0f}μs  "
              f"|  Steady-State Avg: {report['steady_state_avg_us']:>8.0f}μs", flush=True)
        print("", flush=True)

    print(f"📊 Raw results written to: {output_dir}", flush=True)
    print("✅ Benchmark complete.\n", flush=True)


if __name__ == "__main__":
    asyncio.run(main())
