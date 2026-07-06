"""Mock tool executors for the LangGraph baseline.

These tools implement IDENTICAL delay distributions to the Velocity runtime
mock tools, ensuring the benchmark isolates orchestration overhead only:
  - mock_db: 5-15ms jittered delay
  - mock_http: 20-50ms jittered delay
  - mock_file: 1-3ms jittered delay
"""

import asyncio
import json
import random
import time


async def mock_db(operation: str, **kwargs) -> dict:
    """Simulates a database query with 5-15ms jittered delay."""
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


async def mock_http(operation: str, **kwargs) -> dict:
    """Simulates an external API call with 20-50ms jittered delay."""
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


async def mock_file(operation: str, **kwargs) -> dict:
    """Simulates file I/O with 1-3ms jittered delay."""
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
def mock_db_sync(operation: str, **kwargs) -> str:
    """Sync wrapper for mock_db."""
    result = asyncio.get_event_loop().run_until_complete(mock_db(operation, **kwargs))
    return json.dumps(result)


def mock_http_sync(operation: str, **kwargs) -> str:
    """Sync wrapper for mock_http."""
    result = asyncio.get_event_loop().run_until_complete(mock_http(operation, **kwargs))
    return json.dumps(result)


def mock_file_sync(operation: str, **kwargs) -> str:
    """Sync wrapper for mock_file."""
    result = asyncio.get_event_loop().run_until_complete(mock_file(operation, **kwargs))
    return json.dumps(result)
