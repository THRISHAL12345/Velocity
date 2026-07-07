//! Pool size sweep experiment for evaluating worker pool contention.
//!
//! Runs the benchmark across a configurable list of pool sizes at a fixed concurrency level (default 1000).
//! Measures p50/p95/p99 task latency, average queue wait time, and pool construction time.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinHandle;

use velocity_core::scheduler::{Scheduler, TaskResult, ToolCallIntent};
use velocity_core::worker_pool::{WorkerPool, WorkerPoolConfig};
use crate::report::compute_percentiles;

/// Configuration for a pool size sweep experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolSweepConfig {
    pub pool_sizes: Vec<usize>,
    pub fixed_concurrency: usize,
    pub warmup_iterations: usize,
    pub measured_iterations: usize,
    pub profile: String,
}

impl Default for PoolSweepConfig {
    fn default() -> Self {
        Self {
            pool_sizes: vec![64, 256, 1024, 4096],
            fixed_concurrency: 1000,
            warmup_iterations: 5,
            measured_iterations: 50,
            profile: "process_order".to_string(),
        }
    }
}

/// Results from a single pool size run in the sweep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolSweepResult {
    pub pool_size: usize,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub avg_queue_wait_us: u64,
    pub pool_construction_ms: u64,
}

/// Runs the pool size sweep across all configured pool sizes.
pub async fn run_pool_sweep(
    config: PoolSweepConfig,
    tasks: Vec<Vec<ToolCallIntent>>,
) -> Vec<PoolSweepResult> {
    let mut results = Vec::with_capacity(config.pool_sizes.len());

    for &pool_size in &config.pool_sizes {
        let start_construct = Instant::now();
        let pool = Arc::new(WorkerPool::new(WorkerPoolConfig::with_profile(
            pool_size,
            &config.profile,
        )));
        let pool_construction_ms = start_construct.elapsed().as_millis() as u64;
        let scheduler = Arc::new(Scheduler::new(Arc::clone(&pool)));

        // Warm-up phase
        for _ in 0..config.warmup_iterations {
            let s = Arc::clone(&scheduler);
            let task = tasks[0].clone();
            let _ = s.execute_task(task).await;
        }

        // Measured phase
        let mut all_task_times: Vec<u64> = Vec::new();
        for _ in 0..config.measured_iterations {
            let mut handles: Vec<JoinHandle<Result<TaskResult, velocity_core::scheduler::SchedulerError>>> = Vec::with_capacity(tasks.len());
            for task in &tasks {
                let s = Arc::clone(&scheduler);
                let task_clone = task.clone();
                handles.push(tokio::spawn(async move { s.execute_task(task_clone).await }));
            }

            for handle in handles {
                if let Ok(Ok(result)) = handle.await {
                    all_task_times.push(result.total_time_us);
                }
            }
        }

        let stats_percentiles = compute_percentiles(&all_task_times);

        // Compute average queue wait time across all active tools
        let mut total_wait_us = 0u64;
        let mut active_pools = 0u64;
        for tool in &["mock_db", "mock_http", "mock_file", "mock_memory_lookup", "mock_calc_engine", "mock_state_write"] {
            if let Some(stats) = pool.stats(tool) {
                if stats.total > 0 {
                    total_wait_us += stats.avg_wait_us;
                    active_pools += 1;
                }
            }
        }
        let avg_queue_wait_us = if active_pools > 0 {
            total_wait_us / active_pools
        } else {
            0
        };

        results.push(PoolSweepResult {
            pool_size,
            p50_us: stats_percentiles.p50_us,
            p95_us: stats_percentiles.p95_us,
            p99_us: stats_percentiles.p99_us,
            avg_queue_wait_us,
            pool_construction_ms,
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use velocity_core::scheduler::ToolCallIntent;

    #[tokio::test]
    async fn test_pool_sweep_small_concurrency() {
        let config = PoolSweepConfig {
            pool_sizes: vec![4],
            fixed_concurrency: 20,
            warmup_iterations: 1,
            measured_iterations: 2,
            profile: "process_order".to_string(),
        };

        // Create 20 identical tasks each using mock_file (1-3ms delay)
        let mut tasks = Vec::with_capacity(20);
        for _ in 0..20 {
            tasks.push(vec![ToolCallIntent {
                step_id: "step_1".to_string(),
                tool_name: "mock_file".to_string(),
                operation: "read".to_string(),
                args: vec![],
                dependencies: vec![],
            }]);
        }

        let results = run_pool_sweep(config, tasks).await;
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.pool_size, 4);
        assert!(r.p50_us > 0);
        // Because 20 tasks compete for 4 workers sleeping 1-3ms, queue wait must be > 0
        assert!(r.avg_queue_wait_us > 0, "expected avg_queue_wait_us > 0, got {}", r.avg_queue_wait_us);
    }
}
