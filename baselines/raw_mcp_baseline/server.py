"""Raw MCP-style baseline for the Velocity benchmark.

Implements the identical `process_order` task graph using a simulated
MCP-over-stdio pattern: JSON serialization/deserialization on every
tool call hop, with no orchestration framework.

This represents the "no framework" floor — raw JSON-based tool dispatch
with full serialization overhead, matching what a bare MCP server does.

Task graph (identical dependency structure):
  Step 1: mock_db("lookup_account", account_id)     [no deps]
  Step 2: mock_db("check_inventory", sku)            [no deps, independent of 1]
  Step 3: mock_http("get_pricing", sku)              [depends on 2]
  Step 4: mock_db("write_order_record", ...)         [depends on 1 & 3]
  Step 5: mock_file("write_confirmation_log", ...)   [depends on 4]
"""

import asyncio
import json
import random
import statistics
import time
from pathlib import Path


# ─── Mock tools (identical delay distributions) ──────────────────────────────

async def mock_db_raw(operation: str, args: dict) -> dict:
    """Simulates a database query with 5-15ms jittered delay."""
    delay = random.uniform(0.005, 0.015)
    await asyncio.sleep(delay)

    if operation == "lookup_account":
        return {
            "account_id": args.get("account_id", "UNKNOWN"),
            "name": "Test User",
            "balance": 1000.50,
            "status": "active",
        }
    elif operation == "check_inventory":
        return {
            "sku": args.get("sku", "UNKNOWN"),
            "quantity": 42,
            "warehouse": "WH-001",
        }
    elif operation == "write_order_record":
        return {"order_id": "ORD-99001", "status": "confirmed"}
    else:
        raise ValueError(f"unknown db operation: {operation}")


async def mock_http_raw(operation: str, args: dict) -> dict:
    """Simulates an external API call with 20-50ms jittered delay."""
    delay = random.uniform(0.020, 0.050)
    await asyncio.sleep(delay)

    if operation == "get_pricing":
        return {
            "sku": args.get("sku", "UNKNOWN"),
            "unit_price": 29.99,
            "currency": "USD",
            "available": True,
        }
    else:
        raise ValueError(f"unknown http operation: {operation}")


async def mock_file_raw(operation: str, args: dict) -> dict:
    """Simulates file I/O with 1-3ms jittered delay."""
    delay = random.uniform(0.001, 0.003)
    await asyncio.sleep(delay)

    if operation == "write_confirmation_log":
        order_id = args.get("order_id", "UNKNOWN")
        return {
            "file": f"/var/log/orders/{order_id}.log",
            "bytes_written": 256,
            "status": "ok",
        }
    elif operation == "read":
        return {"content": "file contents here", "bytes_read": 128}
    else:
        raise ValueError(f"unknown file operation: {operation}")


# ─── MCP-style JSON serialization layer ──────────────────────────────────────

TOOL_REGISTRY = {
    "mock_db": mock_db_raw,
    "mock_http": mock_http_raw,
    "mock_file": mock_file_raw,
}


async def mcp_tool_call(tool_name: str, operation: str, args: dict) -> dict:
    """Simulates an MCP tool call with full JSON serialization overhead.

    This adds the JSON encode/decode round-trip that a real MCP server
    would perform on every tool call — exactly the overhead Velocity
    aims to eliminate with its binary protocol.
    """
    # Serialize request (as MCP would)
    request_json = json.dumps({
        "tool": tool_name,
        "operation": operation,
        "args": args,
    })

    # Deserialize request (as MCP server would)
    request = json.loads(request_json)

    # Dispatch to tool
    tool_fn = TOOL_REGISTRY.get(request["tool"])
    if tool_fn is None:
        raise ValueError(f"unknown tool: {request['tool']}")

    result = await tool_fn(request["operation"], request["args"])

    # Serialize response (as MCP server would)
    response_json = json.dumps({"result": result, "success": True})

    # Deserialize response (as MCP client would)
    response = json.loads(response_json)

    return response["result"]


# ─── Task execution ──────────────────────────────────────────────────────────

async def process_order_mcp(account_id: str, sku: str) -> dict:
    """Execute process_order with MCP-style serialization on every hop.

    SERIAL execution — no concurrent overlap. This is what raw MCP does:
    each tool call is fully serialized before the next begins.
    """
    start = time.perf_counter_ns()
    step_times = {}

    # Step 1: lookup_account (serial)
    s1_start = time.perf_counter_ns()
    result_1 = await mcp_tool_call("mock_db", "lookup_account", {"account_id": account_id})
    step_times["step_1"] = (time.perf_counter_ns() - s1_start) / 1000

    # Step 2: check_inventory (serial — MCP doesn't overlap)
    s2_start = time.perf_counter_ns()
    result_2 = await mcp_tool_call("mock_db", "check_inventory", {"sku": sku})
    step_times["step_2"] = (time.perf_counter_ns() - s2_start) / 1000

    # Step 3: get_pricing (depends on step 2)
    s3_start = time.perf_counter_ns()
    result_3 = await mcp_tool_call("mock_http", "get_pricing", {"sku": sku})
    step_times["step_3"] = (time.perf_counter_ns() - s3_start) / 1000

    # Step 4: write_order_record (depends on steps 1 & 3)
    s4_start = time.perf_counter_ns()
    result_4 = await mcp_tool_call(
        "mock_db", "write_order_record",
        {"account_id": account_id, "sku": sku}
    )
    step_times["step_4"] = (time.perf_counter_ns() - s4_start) / 1000

    # Step 5: write_confirmation_log (depends on step 4)
    s5_start = time.perf_counter_ns()
    result_5 = await mcp_tool_call(
        "mock_file", "write_confirmation_log",
        {"order_id": result_4.get("order_id", "UNKNOWN")}
    )
    step_times["step_5"] = (time.perf_counter_ns() - s5_start) / 1000

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


# ─── Benchmark harness ──────────────────────────────────────────────────────

async def run_benchmark(concurrency: int, iterations: int, warmup: int) -> dict:
    """Run the benchmark at a given concurrency level."""

    # Warm-up
    for _ in range(warmup):
        await process_order_mcp("WARMUP-ACC", "WARMUP-SKU")

    all_task_times = []
    all_step_times = {f"step_{i}": [] for i in range(1, 6)}
    cold_start_us = 0

    for iteration in range(iterations):
        start = time.perf_counter_ns()

        # Launch concurrent tasks (even MCP can handle concurrent clients)
        tasks = [
            process_order_mcp(f"ACC-{i:05d}", f"SKU-{i:05d}")
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

    steady_times = all_task_times[concurrency:] if len(all_task_times) > concurrency else all_task_times
    steady_state_avg = statistics.mean(steady_times) if steady_times else 0

    return {
        "contender": "raw_mcp",
        "concurrency": concurrency,
        "task_stats": task_stats,
        "step_stats": step_stats,
        "cold_start_us": cold_start_us,
        "steady_state_avg_us": steady_state_avg,
    }


async def main():
    """Run benchmarks at all concurrency levels."""
    concurrency_levels = [1, 10, 100, 1000]
    warmup = 5
    iterations = 50
    output_dir = Path("../../results/raw")
    output_dir.mkdir(parents=True, exist_ok=True)

    print("\n📡 Raw MCP Baseline Benchmark")
    print(f"   Warm-up: {warmup} iterations")
    print(f"   Measured: {iterations} iterations")
    print(f"   Concurrency levels: {concurrency_levels}\n")

    all_reports = []

    for concurrency in concurrency_levels:
        print(f"⏱  Running benchmark at concurrency={concurrency}...")

        report = await run_benchmark(concurrency, iterations, warmup)

        filename = f"raw_mcp_{concurrency}"
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
    print("  RAW MCP BASELINE RESULTS")
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
