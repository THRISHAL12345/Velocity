//! Report generation for benchmark results.
//!
//! Outputs raw results as CSV/JSON and computes percentile statistics
//! using hdrhistogram for accurate p50/p95/p99 capture.

use hdrhistogram::Histogram;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

use crate::load_gen::{BenchConfig, BenchResult};

/// Percentile statistics for latency measurements (all in microseconds).
#[derive(Debug, Clone, Serialize)]
pub struct PercentileStats {
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
    pub min_us: u64,
    pub mean_us: f64,
    pub count: u64,
}

/// Complete benchmark report for a single contender at one concurrency level.
#[derive(Debug, Clone, Serialize)]
pub struct BenchReport {
    pub contender: String,
    pub concurrency: usize,
    pub pool_size: usize,
    pub profile: String,
    pub task_stats: PercentileStats,
    pub step_stats: HashMap<String, PercentileStats>,
    pub cold_start_us: u64,
    pub steady_state_avg_us: u64,
    pub avg_queue_wait_us: u64,
    pub pool_construction_ms: u64,
}

/// Computes percentile statistics from a slice of latency values.
pub fn compute_percentiles(values: &[u64]) -> PercentileStats {
    if values.is_empty() {
        return PercentileStats {
            p50_us: 0,
            p95_us: 0,
            p99_us: 0,
            max_us: 0,
            min_us: 0,
            mean_us: 0.0,
            count: 0,
        };
    }

    let mut hist =
        Histogram::<u64>::new_with_bounds(1, 60_000_000, 3).expect("failed to create histogram");

    for &v in values {
        let val = v.max(1); // hdrhistogram requires >= 1
        hist.record(val).unwrap_or_else(|_| {
            // Value exceeds max — clamp to max
            hist.record(60_000_000).unwrap();
        });
    }

    PercentileStats {
        p50_us: hist.value_at_percentile(50.0),
        p95_us: hist.value_at_percentile(95.0),
        p99_us: hist.value_at_percentile(99.0),
        max_us: hist.max(),
        min_us: hist.min(),
        mean_us: hist.mean(),
        count: hist.len(),
    }
}

/// Generates a BenchReport from raw BenchResult data.
pub fn generate_report(contender: &str, result: &BenchResult, config: &BenchConfig) -> BenchReport {
    let task_stats = compute_percentiles(&result.task_times_us);

    let mut step_stats = HashMap::new();
    for (step_id, times) in &result.step_times_us {
        step_stats.insert(step_id.clone(), compute_percentiles(times));
    }

    BenchReport {
        contender: contender.to_string(),
        concurrency: result.concurrency,
        pool_size: config.pool_size,
        profile: config.profile.clone(),
        task_stats,
        step_stats,
        cold_start_us: result.cold_start_us,
        steady_state_avg_us: result.steady_state_avg_us,
        avg_queue_wait_us: result.avg_queue_wait_us,
        pool_construction_ms: result.pool_construction_ms,
    }
}

/// Writes the benchmark report as a JSON file.
pub fn write_json_report(
    report: &BenchReport,
    output_dir: &Path,
    tag: Option<&str>,
) -> std::io::Result<()> {
    let filename = if let Some(t) = tag {
        format!("{}_{}_{}.json", report.contender, t, report.concurrency)
    } else {
        format!("{}_{}.json", report.contender, report.concurrency)
    };
    let path = output_dir.join(filename);
    let json = serde_json::to_string_pretty(report).map_err(std::io::Error::other)?;
    std::fs::write(path, json)
}

/// Writes the benchmark report as a CSV file.
pub fn write_csv_report(
    report: &BenchReport,
    output_dir: &Path,
    tag: Option<&str>,
) -> std::io::Result<()> {
    let filename = if let Some(t) = tag {
        format!("{}_{}_{}.csv", report.contender, t, report.concurrency)
    } else {
        format!("{}_{}.csv", report.contender, report.concurrency)
    };
    let path = output_dir.join(filename);
    let mut wtr = csv::Writer::from_path(path)?;

    // Header
    wtr.write_record([
        "metric", "p50_us", "p95_us", "p99_us", "max_us", "min_us", "mean_us", "count",
    ])?;

    // Task-level stats
    wtr.write_record([
        "task_total".to_string(),
        report.task_stats.p50_us.to_string(),
        report.task_stats.p95_us.to_string(),
        report.task_stats.p99_us.to_string(),
        report.task_stats.max_us.to_string(),
        report.task_stats.min_us.to_string(),
        format!("{:.2}", report.task_stats.mean_us),
        report.task_stats.count.to_string(),
    ])?;

    // Per-step stats
    let mut step_ids: Vec<&String> = report.step_stats.keys().collect();
    step_ids.sort();
    for step_id in step_ids {
        let stats = &report.step_stats[step_id];
        wtr.write_record([
            step_id.clone(),
            stats.p50_us.to_string(),
            stats.p95_us.to_string(),
            stats.p99_us.to_string(),
            stats.max_us.to_string(),
            stats.min_us.to_string(),
            format!("{:.2}", stats.mean_us),
            stats.count.to_string(),
        ])?;
    }

    wtr.flush()?;
    Ok(())
}

/// Prints a summary table to stdout.
pub fn print_summary(reports: &[BenchReport]) {
    println!("\n{:=<80}", "");
    println!("  VELOCITY BENCHMARK RESULTS");
    println!("{:=<80}\n", "");

    for report in reports {
        println!(
            "Contender: {}  |  Concurrency: {}",
            report.contender, report.concurrency
        );
        println!("{:-<60}", "");
        println!(
            "  Task Total:  p50={:>8}μs  p95={:>8}μs  p99={:>8}μs  max={:>8}μs",
            report.task_stats.p50_us,
            report.task_stats.p95_us,
            report.task_stats.p99_us,
            report.task_stats.max_us
        );
        println!(
            "  Cold Start:  {:>8}μs  |  Steady-State Avg: {:>8}μs",
            report.cold_start_us, report.steady_state_avg_us
        );

        let mut step_ids: Vec<&String> = report.step_stats.keys().collect();
        step_ids.sort();
        for step_id in step_ids {
            let stats = &report.step_stats[step_id];
            println!(
                "  {:<12} p50={:>8}μs  p95={:>8}μs  p99={:>8}μs",
                step_id, stats.p50_us, stats.p95_us, stats.p99_us
            );
        }
        println!();
    }
}
