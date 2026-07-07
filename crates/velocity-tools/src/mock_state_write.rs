//! Mock state write tool executor for sub-millisecond HFT profile.
//!
//! Simulates an in-memory state update or WAL append with a 10–30μs jittered delay.
//!
//! The delay distribution must match identically across all benchmark contenders.

use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for the mock state write executor.
#[derive(Debug, Clone)]
pub struct MockStateWriteConfig {
    /// Minimum simulated latency in microseconds.
    pub min_delay_us: u64,
    /// Maximum simulated latency in microseconds.
    pub max_delay_us: u64,
}

impl Default for MockStateWriteConfig {
    fn default() -> Self {
        Self {
            min_delay_us: 10,
            max_delay_us: 30,
        }
    }
}

/// A mock state write executor that simulates in-memory WAL log/audit operations.
///
/// Designed to be held persistently by a worker — never recreated per call.
#[derive(Debug, Clone)]
pub struct MockStateWrite {
    config: MockStateWriteConfig,
}

impl MockStateWrite {
    /// Creates a new `MockStateWrite` with default configuration (10–30μs jitter).
    pub fn new() -> Self {
        Self {
            config: MockStateWriteConfig::default(),
        }
    }

    /// Samples a random delay in microseconds according to the configured distribution.
    pub fn sample_delay_us(&self) -> u64 {
        let mut rng = rand::thread_rng();
        rng.gen_range(self.config.min_delay_us..=self.config.max_delay_us)
    }

    /// Executes a mock state write operation.
    pub async fn execute(&self, operation: &str, args: &[(String, String)]) -> Result<String, String> {
        let delay = self.sample_delay_us();
        sleep(Duration::from_micros(delay)).await;

        match operation {
            "log_audit" => {
                let trade_id = args
                    .iter()
                    .find(|(k, _)| k == "trade_id")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("UNKNOWN");
                Ok(format!(
                    r#"{{"file":"/var/log/hft/{}.log","bytes_written":128,"status":"ok"}}"#,
                    trade_id
                ))
            }
            _ => Err(format!("unknown state write operation: {}", operation)),
        }
    }
}

impl Default for MockStateWrite {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn test_delay_distribution_bounds() {
        let tool = MockStateWrite::new();
        // Assert sampled delays fall strictly within the 10–30μs range per §4.5
        for _ in 0..100 {
            let delay = tool.sample_delay_us();
            assert!(delay >= 10, "expected delay >= 10us, got {}us", delay);
            assert!(delay <= 30, "expected delay <= 30us, got {}us", delay);
        }
    }

    #[tokio::test]
    async fn test_execution_latency() {
        let tool = MockStateWrite::new();
        let start = Instant::now();
        let _ = tool.execute("log_audit", &[("trade_id".to_string(), "TRD-1".to_string())]).await.unwrap();
        let elapsed_us = start.elapsed().as_micros() as u64;
        assert!(elapsed_us < 50_000, "expected execution under 50ms, got {}us", elapsed_us);
    }
}
