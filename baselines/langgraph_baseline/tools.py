"""Mock tool executors for the LangGraph baseline.

These tools implement IDENTICAL delay distributions to the Velocity runtime
mock tools, ensuring the benchmark isolates orchestration overhead only:
  - process_order: mock_db (5-15ms), mock_http (20-50ms), mock_file (1-3ms)
  - hft_tick: mock_db (50-150μs), mock_http (200-500μs), mock_file (10-30μs)
"""

import asyncio
import json
import random
import time


async def mock_db(operation: str, profile: str = "process_order", **kwargs) -> dict:
    """Simulates a database query."""
    if profile == "hft_tick":
        delay = random.uniform(0.000050, 0.000150)
        await asyncio.sleep(delay)
        if operation == "lookup_orderbook":
            return {"symbol": kwargs.get("symbol", "UNKNOWN"), "bids": 10, "asks": 10}
        elif operation == "check_risk_limit":
            return {"account_id": kwargs.get("account_id", "UNKNOWN"), "limit_ok": True, "margin": 100000}
        elif operation == "write_trade_record":
            return {"trade_id": "TRD-1001", "status": "confirmed"}
        else:
            raise ValueError(f"unknown hft db operation: {operation}")
    else:
        delay = random.uniform(0.005, 0.015)
        await asyncio.sleep(delay)
        if operation == "lookup_account":
            account_id = kwargs.get("account_id", "UNKNOWN")
            return {
                "account_id": account_id,
                "name": "Test User",
                "balance": 1000.50,
                "status": "active",
            }
        elif operation == "check_inventory":
            sku = kwargs.get("sku", "UNKNOWN")
            return {"sku": sku, "quantity": 42, "warehouse": "WH-001"}
        elif operation == "write_order_record":
            return {"order_id": "ORD-99001", "status": "confirmed"}
        else:
            raise ValueError(f"unknown db operation: {operation}")


async def mock_http(operation: str, profile: str = "process_order", **kwargs) -> dict:
    """Simulates an external API call."""
    if profile == "hft_tick":
        delay = random.uniform(0.000200, 0.000500)
        await asyncio.sleep(delay)
        if operation == "calculate_alpha":
            return {"symbol": kwargs.get("symbol", "UNKNOWN"), "alpha_score": 0.85, "confidence": 0.92}
        else:
            raise ValueError(f"unknown hft http operation: {operation}")
    else:
        delay = random.uniform(0.020, 0.050)
        await asyncio.sleep(delay)
        if operation == "get_pricing":
            sku = kwargs.get("sku", "UNKNOWN")
            return {
                "sku": sku,
                "unit_price": 29.99,
                "currency": "USD",
                "available": True,
            }
        else:
            raise ValueError(f"unknown http operation: {operation}")


async def mock_file(operation: str, profile: str = "process_order", **kwargs) -> dict:
    """Simulates file I/O."""
    if profile == "hft_tick":
        delay = random.uniform(0.000010, 0.000030)
        await asyncio.sleep(delay)
        if operation == "log_audit":
            return {"file": "/var/log/hft/audit.log", "bytes_written": 64, "status": "ok"}
        else:
            raise ValueError(f"unknown hft file operation: {operation}")
    else:
        delay = random.uniform(0.001, 0.003)
        await asyncio.sleep(delay)
        if operation == "write_confirmation_log":
            order_id = kwargs.get("order_id", "UNKNOWN")
            return {
                "file": f"/var/log/orders/{order_id}.log",
                "bytes_written": 256,
                "status": "ok",
            }
        elif operation == "read":
            return {"content": "file contents here", "bytes_read": 128}
        else:
            raise ValueError(f"unknown file operation: {operation}")


# Synchronous wrappers for LangGraph tool compatibility
def mock_db_sync(operation: str, profile: str = "process_order", **kwargs) -> str:
    """Sync wrapper for mock_db."""
    result = asyncio.get_event_loop().run_until_complete(mock_db(operation, profile=profile, **kwargs))
    return json.dumps(result)


def mock_http_sync(operation: str, profile: str = "process_order", **kwargs) -> str:
    """Sync wrapper for mock_http."""
    result = asyncio.get_event_loop().run_until_complete(mock_http(operation, profile=profile, **kwargs))
    return json.dumps(result)


def mock_file_sync(operation: str, profile: str = "process_order", **kwargs) -> str:
    """Sync wrapper for mock_file."""
    result = asyncio.get_event_loop().run_until_complete(mock_file(operation, profile=profile, **kwargs))
    return json.dumps(result)
