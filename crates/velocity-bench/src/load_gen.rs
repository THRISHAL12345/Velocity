//! Async load generator for running benchmarks at various concurrency levels.
//!
//! Supports configurable concurrency: 1, 10, 100, 1000 simultaneous task executions.

use std::sync::Arc;
use std::time::Instant;

use velocity_core::scheduler::{Scheduler, TaskResult, ToolCallIntent};
use velocity_core::worker_pool::{WorkerPool, WorkerPoolConfig};

/// Configuration for a single benchmark run.
#[derive(Debug, Clone)]
pub struct BenchConfig {
    /// Number of concurrent tasks to execute.
    pub concurrency: usize,
    /// Number of workers per tool type in the pool.
    pub pool_size: usize,
    /// Number of warm-up iterations before measurement.
    pub warmup_iterations: usize,
    /// Number of measured iterations.
    pub measured_iterations: usize,
    /// Task profile name ("process_order" or "hft_tick").
    pub profile: String,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            concurrency: 1,
            pool_size: 64,
            warmup_iterations: 5,
            measured_iterations: 50,
            profile: "process_order".to_string(),
        }
    }
}

/// Results from a single benchmark run.
#[derive(Debug, Clone)]
pub struct BenchResult {
    /// The concurrency level for this run.
    pub concurrency: usize,
    /// Total task completion times in microseconds, one per task.
    pub task_times_us: Vec<u64>,
    /// Per-step latencies in microseconds, keyed by step ID.
    pub step_times_us: std::collections::HashMap<String, Vec<u64>>,
    /// Cold-start time (first task) in microseconds.
    pub cold_start_us: u64,
    /// Steady-state average time (excluding cold start) in microseconds.
    pub steady_state_avg_us: u64,
}

/// Runs the Velocity benchmark at a given concurrency level.
///
/// Executes the task graph `concurrency` times concurrently,
/// collecting per-call and total-task timing data.
pub async fn run_velocity_benchmark(
    config: &BenchConfig,
    tasks: Vec<Vec<ToolCallIntent>>,
) -> BenchResult {
    let pool = Arc::new(WorkerPool::new(WorkerPoolConfig::with_profile(
        config.pool_size,
        &config.profile,
    )));
    let scheduler = Arc::new(Scheduler::new(Arc::clone(&pool)));

    // Warm-up phase: run a few iterations without measuring
    for _ in 0..config.warmup_iterations {
        let s = Arc::clone(&scheduler);
        let task = tasks[0].clone();
        let _ = s.execute_task(task).await;
    }

    // Measured phase: run all tasks concurrently
    let mut all_task_times: Vec<u64> = Vec::new();
    let mut all_step_times: std::collections::HashMap<String, Vec<u64>> =
        std::collections::HashMap::new();
    let mut cold_start_us = 0u64;

    for iteration in 0..config.measured_iterations {
        let start = Instant::now();

        // Launch all concurrent tasks
        let mut handles = Vec::with_capacity(tasks.len());
        for task in &tasks {
            let s = Arc::clone(&scheduler);
            let task_clone = task.clone();
            handles.push(tokio::spawn(async move { s.execute_task(task_clone).await }));
        }

        // Collect results
        let mut results: Vec<TaskResult> = Vec::with_capacity(handles.len());
        for handle in handles {
            match handle.await {
                Ok(Ok(result)) => results.push(result),
                Ok(Err(e)) => eprintln!("task error: {}", e),
                Err(e) => eprintln!("join error: {}", e),
            }
        }

        let batch_time_us = start.elapsed().as_micros() as u64;

        // Record cold start (first iteration)
        if iteration == 0 {
            cold_start_us = batch_time_us;
        }

        // Collect per-task times
        for result in &results {
            all_task_times.push(result.total_time_us);

            for (step_id, step_result) in &result.step_results {
                let step_time = step_result
                    .trace
                    .total_us()
                    .unwrap_or(0);
                all_step_times
                    .entry(step_id.clone())
                    .or_default()
                    .push(step_time);
            }
        }
    }

    // Compute steady-state average (excluding first iteration)
    let steady_state_avg_us = if all_task_times.len() > tasks.len() {
        let steady_times: Vec<u64> = all_task_times.iter().skip(tasks.len()).copied().collect();
        steady_times.iter().sum::<u64>() / steady_times.len().max(1) as u64
    } else {
        all_task_times.iter().sum::<u64>() / all_task_times.len().max(1) as u64
    };

    BenchResult {
        concurrency: config.concurrency,
        task_times_us: all_task_times,
        step_times_us: all_step_times,
        cold_start_us,
        steady_state_avg_us,
    }
}
