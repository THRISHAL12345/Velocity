//! Mock calculation engine tool executor for sub-millisecond HFT profile.
//!
//! Simulates a risk or pricing calculation engine with a 200–500μs jittered delay.
//!
//! The delay distribution must match identically across all benchmark contenders.

use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for the mock calculation engine executor.
#[derive(Debug, Clone)]
pub struct MockCalcEngineConfig {
    /// Minimum simulated latency in microseconds.
    pub min_delay_us: u64,
    /// Maximum simulated latency in microseconds.
    pub max_delay_us: u64,
}

impl Default for MockCalcEngineConfig {
    fn default() -> Self {
        Self {
            min_delay_us: 200,
            max_delay_us: 500,
        }
    }
}

/// A mock calculation engine executor that simulates pricing/alpha calculation operations.
///
/// Designed to be held persistently by a worker — never recreated per call.
#[derive(Debug, Clone)]
pub struct MockCalcEngine {
    config: MockCalcEngineConfig,
}

impl MockCalcEngine {
    /// Creates a new `MockCalcEngine` with default configuration (200–500μs jitter).
    pub fn new() -> Self {
        Self {
            config: MockCalcEngineConfig::default(),
        }
    }

    /// Samples a random delay in microseconds according to the configured distribution.
    pub fn sample_delay_us(&self) -> u64 {
        let mut rng = rand::thread_rng();
        rng.gen_range(self.config.min_delay_us..=self.config.max_delay_us)
    }

    /// Executes a mock calculation operation.
    pub async fn execute(
        &self,
        operation: &str,
        args: &[(String, String)],
    ) -> Result<String, String> {
        let delay = self.sample_delay_us();
        sleep(Duration::from_micros(delay)).await;

        match operation {
            "calculate_alpha" => {
                let symbol = args
                    .iter()
                    .find(|(k, _)| k == "symbol")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("UNKNOWN");
                Ok(format!(
                    r#"{{"symbol":"{}","alpha_score":0.85,"signal":"BUY"}}"#,
                    symbol
                ))
            }
            _ => Err(format!("unknown calc engine operation: {}", operation)),
        }
    }
}

impl Default for MockCalcEngine {
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
        let tool = MockCalcEngine::new();
        // Assert sampled delays fall strictly within the 200–500μs range per §4.5
        for _ in 0..100 {
            let delay = tool.sample_delay_us();
            assert!(delay >= 200, "expected delay >= 200us, got {}us", delay);
            assert!(delay <= 500, "expected delay <= 500us, got {}us", delay);
        }
    }

    #[tokio::test]
    async fn test_execution_latency() {
        let tool = MockCalcEngine::new();
        let start = Instant::now();
        let _ = tool
            .execute(
                "calculate_alpha",
                &[("symbol".to_string(), "BTC-USD".to_string())],
            )
            .await
            .unwrap();
        let elapsed_us = start.elapsed().as_micros() as u64;
        assert!(
            elapsed_us < 50_000,
            "expected execution under 50ms, got {}us",
            elapsed_us
        );
    }
}
