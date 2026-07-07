//! Task definitions for the benchmark harness.
//!
//! Contains the `process_order` task and utilities for constructing
//! benchmark workloads at various concurrency levels.

use velocity_core::scheduler::{hft_tick_task, process_order_task, ToolCallIntent};

/// Creates a single `process_order` task with the given parameters.
pub fn create_process_order(account_id: &str, sku: &str) -> Vec<ToolCallIntent> {
    process_order_task(account_id, sku)
}

/// Creates N independent `process_order` tasks for concurrent benchmarking.
///
/// Each task gets a unique account_id to avoid confusion in the results.
pub fn create_concurrent_tasks(count: usize) -> Vec<Vec<ToolCallIntent>> {
    (0..count)
        .map(|i| {
            create_process_order(
                &format!("ACC-{:05}", i),
                &format!("SKU-{:05}", i),
            )
        })
        .collect()
}

/// Creates N independent `hft_tick` tasks for concurrent low-latency benchmarking.
pub fn create_hft_tasks(count: usize) -> Vec<Vec<ToolCallIntent>> {
    (0..count)
        .map(|i| {
            hft_tick_task(
                &format!("SYM-{:05}", i),
                &format!("TRADER-{:05}", i),
            )
        })
        .collect()
}
