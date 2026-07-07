#!/usr/bin/env python3
"""Raw MCP baseline with fair bounded concurrency (semaphore cap) per AGENTS-v1.md §5.

Simulates standard MCP tool execution where every tool call incurs:
1. JSON serialization of request
2. JSON deserialization on server
3. Tool execution (behind an asyncio.Semaphore to bound concurrent in-flight calls)
4. JSON serialization of response
5. JSON deserialization on client
"""

import argparse
import asyncio
import json
import random
import sys
import time
from pathlib import Path


# ─── Mock tools (identical delay distributions) ──────────────────────────────

async def mock_db_raw(operation: str, args: dict, profile: str = "process_order") -> dict:
    """Simulates a database query."""
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


async def mock_http_raw(operation: str, args: dict, profile: str = "process_order") -> dict:
    """Simulates an external API call."""
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


async def mock_file_raw(operation: str, args: dict, profile: str = "process_order") -> dict:
    """Simulates file I/O."""
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


async def mock_memory_lookup_raw(operation: str, args: dict, profile: str = "process_order") -> dict:
    """Simulates an ultra-fast in-memory lookup (50-150μs)."""
    delay = random.uniform(0.000050, 0.000150)
    await asyncio.sleep(delay)
    if operation == "lookup_orderbook":
        return {"symbol": args.get("symbol", "UNKNOWN"), "bids": 10, "asks": 10}
    elif operation == "check_risk_limit":
        return {"account_id": args.get("account_id", "UNKNOWN"), "limit_ok": True, "margin": 100000}
    elif operation == "write_trade_record":
        return {"trade_id": "TRD-1001", "status": "confirmed"}
    else:
        raise ValueError(f"unknown memory_lookup operation: {operation}")


async def mock_calc_engine_raw(operation: str, args: dict, profile: str = "process_order") -> dict:
    """Simulates a low-latency calculation engine (200-500μs)."""
    delay = random.uniform(0.000200, 0.000500)
    await asyncio.sleep(delay)
    if operation == "calculate_alpha":
        return {"symbol": args.get("symbol", "UNKNOWN"), "alpha_score": 0.85, "confidence": 0.92}
    else:
        raise ValueError(f"unknown calc_engine operation: {operation}")


async def mock_state_write_raw(operation: str, args: dict, profile: str = "process_order") -> dict:
    """Simulates an ultra-fast state update (10-30μs)."""
    delay = random.uniform(0.000010, 0.000030)
    await asyncio.sleep(delay)
    if operation == "log_audit":
        return {"file": "/var/log/hft/audit.log", "bytes_written": 64, "status": "ok"}
    else:
        raise ValueError(f"unknown state_write operation: {operation}")


# ─── MCP-style JSON serialization layer with Semaphore Cap ───────────────────

TOOL_REGISTRY = {
    "mock_db": mock_db_raw,
    "mock_http": mock_http_raw,
    "mock_file": mock_file_raw,
    "mock_memory_lookup": mock_memory_lookup_raw,
    "mock_calc_engine": mock_calc_engine_raw,
    "mock_state_write": mock_state_write_raw,
}


async def _mcp_tool_call_inner(tool_name: str, operation: str, args: dict, profile: str) -> dict:
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

    result = await tool_fn(request["operation"], request["args"], profile)

    # Serialize response (as MCP server would)
    response_json = json.dumps({"result": result, "success": True})

    # Deserialize response (as MCP client would)
    response = json.loads(response_json)

    return response["result"]


async def mcp_tool_call_capped(tool_name: str, operation: str, args: dict, profile: str, semaphore: asyncio.Semaphore) -> dict:
    """Simulates an MCP tool call wrapped in a bounded resource semaphore."""
    async with semaphore:
        return await _mcp_tool_call_inner(tool_name, operation, args, profile)


# ─── Task execution ──────────────────────────────────────────────────────────

async def execute_task_mcp_capped(task_id: int, profile: str, semaphore: asyncio.Semaphore) -> dict:
    """Execute task graph with MCP-style serialization and bounded concurrency."""
    start = time.perf_counter_ns()
    step_times = {}

    if profile == "hft_tick":
        symbol = f"SYM-{task_id:05d}"
        account_id = f"TRADER-{task_id:05d}"

        # Step 1: lookup_orderbook
        s1_start = time.perf_counter_ns()
        result_1 = await mcp_tool_call_capped("mock_memory_lookup", "lookup_orderbook", {"symbol": symbol}, profile, semaphore)
        step_times["step_1"] = (time.perf_counter_ns() - s1_start) / 1000

        # Step 2: check_risk_limit
        s2_start = time.perf_counter_ns()
        result_2 = await mcp_tool_call_capped("mock_memory_lookup", "check_risk_limit", {"account_id": account_id}, profile, semaphore)
        step_times["step_2"] = (time.perf_counter_ns() - s2_start) / 1000

        # Step 3: calculate_alpha
        s3_start = time.perf_counter_ns()
        result_3 = await mcp_tool_call_capped("mock_calc_engine", "calculate_alpha", {"symbol": symbol}, profile, semaphore)
        step_times["step_3"] = (time.perf_counter_ns() - s3_start) / 1000

        # Step 4: write_trade_record
        s4_start = time.perf_counter_ns()
        result_4 = await mcp_tool_call_capped(
            "mock_memory_lookup", "write_trade_record",
            {"symbol": symbol, "account_id": account_id}, profile, semaphore
        )
        step_times["step_4"] = (time.perf_counter_ns() - s4_start) / 1000

        # Step 5: log_audit
        s5_start = time.perf_counter_ns()
        result_5 = await mcp_tool_call_capped("mock_state_write", "log_audit", {"trade_id": "TRD-1001"}, profile, semaphore)
        step_times["step_5"] = (time.perf_counter_ns() - s5_start) / 1000

        results = {
            "step_1": result_1, "step_2": result_2, "step_3": result_3,
            "step_4": result_4, "step_5": result_5,
        }
    else:
        account_id = f"ACC-{task_id:05d}"
        sku = f"SKU-{task_id:05d}"

        # Step 1: lookup_account
        s1_start = time.perf_counter_ns()
        result_1 = await mcp_tool_call_capped("mock_db", "lookup_account", {"account_id": account_id}, profile, semaphore)
        step_times["step_1"] = (time.perf_counter_ns() - s1_start) / 1000

        # Step 2: check_inventory
        s2_start = time.perf_counter_ns()
        result_2 = await mcp_tool_call_capped("mock_db", "check_inventory", {"sku": sku}, profile, semaphore)
        step_times["step_2"] = (time.perf_counter_ns() - s2_start) / 1000

        # Step 3: get_pricing
        s3_start = time.perf_counter_ns()
        result_3 = await mcp_tool_call_capped("mock_http", "get_pricing", {"sku": sku}, profile, semaphore)
        step_times["step_3"] = (time.perf_counter_ns() - s3_start) / 1000

        # Step 4: write_order_record
        s4_start = time.perf_counter_ns()
        result_4 = await mcp_tool_call_capped(
            "mock_db", "write_order_record",
            {"account_id": account_id, "sku": sku}, profile, semaphore
        )
        step_times["step_4"] = (time.perf_counter_ns() - s4_start) / 1000

        # Step 5: write_confirmation_log
        s5_start = time.perf_counter_ns()
        order_id = result_4.get("order_id", "UNKNOWN")
        result_5 = await mcp_tool_call_capped("mock_file", "write_confirmation_log", {"order_id": order_id}, profile, semaphore)
        step_times["step_5"] = (time.perf_counter_ns() - s5_start) / 1000

        results = {
            "step_1": result_1, "step_2": result_2, "step_3": result_3,
            "step_4": result_4, "step_5": result_5,
        }

    total_us = (time.perf_counter_ns() - start) / 1000
    return {
        "task_id": task_id,
        "total_us": total_us,
        "step_times": step_times,
        "results": results,
    }


# ─── Benchmark driver ────────────────────────────────────────────────────────

def compute_percentile(values: list, p: float) -> float:
    if not values:
        return 0.0
    sorted_vals = sorted(values)
    idx = int(len(sorted_vals) * (p / 100.0))
    idx = min(idx, len(sorted_vals) - 1)
    return sorted_vals[idx]


async def run_benchmark(concurrency: int, iterations: int, warmup: int, profile: str, pool_size: int) -> dict:
    """Run benchmark for a specific concurrency level with semaphore cap."""
    semaphore = asyncio.Semaphore(pool_size)

    # Warmup
    for i in range(warmup):
        await execute_task_mcp_capped(i, profile, semaphore)

    # Measured runs
    task_times = []
    step_times = {"step_1": [], "step_2": [], "step_3": [], "step_4": [], "step_5": []}

    for batch_idx in range(iterations):
        tasks = [
            execute_task_mcp_capped(batch_idx * concurrency + i, profile, semaphore)
            for i in range(concurrency)
        ]
        results = await asyncio.gather(*tasks)

        for r in results:
            task_times.append(r["total_us"])
            for step_id, st in r["step_times"].items():
                if step_id in step_times:
                    step_times[step_id].append(st)

    # Calculate statistics
    task_stats = {
        "p50_us": compute_percentile(task_times, 50),
        "p95_us": compute_percentile(task_times, 95),
        "p99_us": compute_percentile(task_times, 99),
        "max_us": max(task_times) if task_times else 0,
        "min_us": min(task_times) if task_times else 0,
        "mean_us": sum(task_times) / len(task_times) if task_times else 0,
        "count": len(task_times),
    }

    step_stats = {}
    for step_id, times in step_times.items():
        step_stats[step_id] = {
            "p50_us": compute_percentile(times, 50),
            "p95_us": compute_percentile(times, 95),
            "p99_us": compute_percentile(times, 99),
            "max_us": max(times) if times else 0,
            "min_us": min(times) if times else 0,
            "mean_us": sum(times) / len(times) if times else 0,
            "count": len(times),
        }

    cold_start = task_times[0] if task_times else 0
    steady_state = sum(task_times[1:]) / len(task_times[1:]) if len(task_times) > 1 else cold_start

    return {
        "contender": "raw_mcp_capped",
        "pool_size": pool_size,
        "concurrency": concurrency,
        "profile": profile,
        "warmup_iterations": warmup,
        "measured_iterations": iterations,
        "task_stats": task_stats,
        "step_stats": step_stats,
        "cold_start_us": cold_start,
        "steady_state_avg_us": steady_state,
    }


async def main():
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")

    parser = argparse.ArgumentParser(description="Raw MCP Capped Baseline Benchmark")
    parser.add_argument("--concurrency", default="1000", help="Comma-separated concurrency levels")
    parser.add_argument("--profile", default="process_order", help="Task profile (process_order or hft_tick)")
    parser.add_argument("--pool-size", type=int, default=64, help="Bounded concurrency limit (semaphore cap)")
    args = parser.parse_args()

    concurrency_levels = [int(x.strip()) for x in args.concurrency.split(',') if x.strip()]
    warmup = 5
    iterations = 50
    output_dir = Path(__file__).parent.parent.parent / "results" / "raw"
    output_dir.mkdir(parents=True, exist_ok=True)

    print("\n📡 Raw MCP Capped Baseline Benchmark")
    print(f"   Profile: {args.profile}")
    print(f"   Pool size (semaphore cap): {args.pool_size}")
    print(f"   Warm-up: {warmup} iterations")
    print(f"   Measured: {iterations} iterations")
    print(f"   Concurrency levels: {concurrency_levels}\n")

    all_reports = []

    for concurrency in concurrency_levels:
        print(f"⏱  Running benchmark at concurrency={concurrency}...")

        report = await run_benchmark(concurrency, iterations, warmup, args.profile, args.pool_size)

        if args.profile == "hft_tick":
            filename = f"raw_mcp_capped{args.pool_size}_hft_{concurrency}"
        else:
            filename = f"raw_mcp_capped{args.pool_size}_{concurrency}"

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
            f"p99={report['task_stats']['p99_us']:.0f}μs"
        )

        all_reports.append(report)

    # Print summary
    print(f"\n{'=' * 80}")
    print("  RAW MCP CAPPED BASELINE RESULTS")
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
