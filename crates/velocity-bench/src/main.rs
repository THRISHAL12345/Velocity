//! Velocity benchmark harness — the most important deliverable of the MVP.
//!
//! Runs the `process_order` task across the Velocity runtime at multiple
//! concurrency levels, collecting p50/p95/p99/max latency data and
//! outputting results as CSV/JSON for comparison with baselines.

mod load_gen;
mod pool_sweep;
mod report;
mod task_definitions;

use clap::Parser;
use std::path::PathBuf;

use load_gen::{run_velocity_benchmark, BenchConfig};
use report::{generate_report, print_summary, write_csv_report, write_json_report};
use task_definitions::{create_concurrent_tasks, create_hft_tasks};

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

    /// Task profile to benchmark ("process_order" or "hft_tick")
    #[arg(long, default_value = "process_order")]
    profile: String,

    /// Pool sizes to sweep (comma-separated, e.g., "64,256,1024,4096"). Overrides --pool-size if set.
    #[arg(long)]
    sweep_pool_sizes: Option<String>,
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

    // Parse pool sizes
    let pool_sizes: Vec<usize> = if let Some(ref sweep) = args.sweep_pool_sizes {
        sweep
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    } else {
        vec![args.pool_size]
    };
    let is_sweep = args.sweep_pool_sizes.is_some() || pool_sizes.len() > 1;

    // Ensure output directory exists
    std::fs::create_dir_all(&args.output_dir).expect("failed to create output directory");

    println!("\n🚀 Velocity Benchmark Harness");
    println!("   Profile: {}", args.profile);
    println!("   Pool sizes: {:?} workers/tool", pool_sizes);
    println!("   Warm-up: {} iterations", args.warmup);
    println!("   Measured: {} iterations", args.iterations);
    println!("   Concurrency levels: {:?}\n", concurrency_levels);

    let mut all_reports = Vec::new();

    for &pool_size in &pool_sizes {
        for &concurrency in &concurrency_levels {
            println!(
                "⏱  Running benchmark [profile={}, pool={}, concurrency={}]...",
                args.profile, pool_size, concurrency
            );

            let tasks = if args.profile == "hft_tick" {
                create_hft_tasks(concurrency)
            } else {
                create_concurrent_tasks(concurrency)
            };

            let config = BenchConfig {
                concurrency,
                pool_size,
                warmup_iterations: args.warmup,
                measured_iterations: args.iterations,
                profile: args.profile.clone(),
            };

            let result = run_velocity_benchmark(&config, tasks).await;
            let report = generate_report("velocity", &result, &config);

            let tag: Option<String> = match (is_sweep, args.profile.as_str()) {
                (true, "hft_tick") => Some(format!("p{}_hft", pool_size)),
                (true, _) => Some(format!("p{}", pool_size)),
                (false, "hft_tick") => Some("hft".to_string()),
                (false, _) => None,
            };

            // Write raw results
            if let Err(e) = write_json_report(&report, &args.output_dir, tag.as_deref()) {
                eprintln!("   ⚠ Failed to write JSON: {}", e);
            }
            if let Err(e) = write_csv_report(&report, &args.output_dir, tag.as_deref()) {
                eprintln!("   ⚠ Failed to write CSV: {}", e);
            }

            if is_sweep {
                let sweep_res = task_definitions::PoolSweepResult {
                    pool_size,
                    p50_us: report.task_stats.p50_us,
                    p95_us: report.task_stats.p95_us,
                    p99_us: report.task_stats.p99_us,
                    avg_queue_wait_us: report.avg_queue_wait_us,
                    pool_construction_ms: report.pool_construction_ms,
                };
                let sweep_file = args.output_dir.join(format!("pool_sweep_{}.json", pool_size));
                if let Ok(json_str) = serde_json::to_string_pretty(&sweep_res) {
                    let _ = std::fs::write(sweep_file, json_str);
                }
            }

            println!(
                "   ✓ [pool={}, conc={}]: p50={}μs  p95={}μs  p99={}μs",
                pool_size,
                concurrency,
                report.task_stats.p50_us,
                report.task_stats.p95_us,
                report.task_stats.p99_us
            );

            all_reports.push(report);
        }
    }

    // Print full summary
    print_summary(&all_reports);

    println!("📊 Raw results written to: {}", args.output_dir.display());
    println!("✅ Benchmark complete.\n");
}
