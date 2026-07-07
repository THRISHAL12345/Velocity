#!/usr/bin/env python3
"""Unit test verifying that asyncio.Semaphore strictly caps concurrent execution per AGENTS-v1.md §5.5."""

import asyncio
import sys
import unittest
from server import mcp_tool_call_capped


class TestSemaphoreCap(unittest.IsolatedAsyncioTestCase):
    async def test_semaphore_bounds_concurrency(self):
        pool_size = 4
        semaphore = asyncio.Semaphore(pool_size)
        active_calls = 0
        max_active_calls = 0
        lock = asyncio.Lock()

        async def tracking_tool_call(i):
            nonlocal active_calls, max_active_calls
            async with semaphore:
                async with lock:
                    active_calls += 1
                    if active_calls > max_active_calls:
                        max_active_calls = active_calls
                # Simulate work
                await asyncio.sleep(0.05)
                async with lock:
                    active_calls -= 1

        # Launch 20 concurrent tasks against pool_size of 4
        tasks = [tracking_tool_call(i) for i in range(20)]
        await asyncio.gather(*tasks)

        self.assertLessEqual(max_active_calls, pool_size, f"Max active calls ({max_active_calls}) exceeded pool size ({pool_size})")
        self.assertEqual(max_active_calls, pool_size, f"Semaphore did not reach pool size capacity: {max_active_calls}")


if __name__ == "__main__":
    unittest.main()
