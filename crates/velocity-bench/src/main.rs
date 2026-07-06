//! Velocity benchmark harness — the most important deliverable of the MVP.
//!
//! Runs the `process_order` task across the Velocity runtime at multiple
//! concurrency levels, collecting p50/p95/p99/max latency data and
//! outputting results as CSV/JSON for comparison with baselines.

mod load_gen;
mod report;
mod task_definitions;

use clap::Parser;
use std::path::PathBuf;

use load_gen::{run_velocity_benchmark, BenchConfig};
use report::{generate_report, print_summary, write_csv_report, write_json_report};
use task_definitions::create_concurrent_tasks;

/// Velocity Benchmark Harness
#[derive(Parser, Debug)]
#[command(name = "velocity-bench")]
#[command(about = "Benchmark harness for the Velocity low-latency runtime")]
struct Args {
    /// Output directory for results (default: results/raw)
    #[arg(short, long, default_value = "results/raw")]
    output_dir: PathBuf,

    /// Concurrency levels to test (comma-separated)
    #[arg(short, long, default_value = "1,10,100,1000")]
    concurrency: String,

    /// Workers per tool type in the pool
    #[arg(short, long, default_value = "64")]
    pool_size: usize,

    /// Warm-up iterations before measurement
    #[arg(short, long, default_value = "5")]
    warmup: usize,

    /// Number of measured iterations
    #[arg(short, long, default_value = "50")]
    iterations: usize,
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .init();

    let args = Args::parse();

    // Parse concurrency levels
    let concurrency_levels: Vec<usize> = args
        .concurrency
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    // Ensure output directory exists
    std::fs::create_dir_all(&args.output_dir).expect("failed to create output directory");

    println!("\n🚀 Velocity Benchmark Harness");
    println!("   Pool size: {} workers/tool", args.pool_size);
    println!("   Warm-up: {} iterations", args.warmup);
    println!("   Measured: {} iterations", args.iterations);
    println!("   Concurrency levels: {:?}\n", concurrency_levels);

    let mut all_reports = Vec::new();

    for &concurrency in &concurrency_levels {
        println!(
            "⏱  Running benchmark at concurrency={}...",
            concurrency
        );

        let tasks = create_concurrent_tasks(concurrency);

        let config = BenchConfig {
            concurrency,
            pool_size: args.pool_size,
            warmup_iterations: args.warmup,
            measured_iterations: args.iterations,
        };

        let result = run_velocity_benchmark(&config, tasks).await;
        let report = generate_report("velocity", &result);

        // Write raw results
        if let Err(e) = write_json_report(&report, &args.output_dir) {
            eprintln!("   ⚠ Failed to write JSON: {}", e);
        }
        if let Err(e) = write_csv_report(&report, &args.output_dir) {
            eprintln!("   ⚠ Failed to write CSV: {}", e);
        }

        println!(
            "   ✓ concurrency={}: p50={}μs  p95={}μs  p99={}μs",
            concurrency,
            report.task_stats.p50_us,
            report.task_stats.p95_us,
            report.task_stats.p99_us
        );

        all_reports.push(report);
    }

    // Print full summary
    print_summary(&all_reports);

    println!("📊 Raw results written to: {}", args.output_dir.display());
    println!("✅ Benchmark complete.\n");
}
