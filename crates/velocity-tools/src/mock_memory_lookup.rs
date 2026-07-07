//! Mock memory lookup tool executor for sub-millisecond HFT profile.
//!
//! Simulates an in-memory orderbook or feature lookup with a 50–150μs jittered delay.
//!
//! The delay distribution must match identically across all benchmark contenders.

use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for the mock memory lookup executor.
#[derive(Debug, Clone)]
pub struct MockMemoryLookupConfig {
    /// Minimum simulated latency in microseconds.
    pub min_delay_us: u64,
    /// Maximum simulated latency in microseconds.
    pub max_delay_us: u64,
}

impl Default for MockMemoryLookupConfig {
    fn default() -> Self {
        Self {
            min_delay_us: 50,
            max_delay_us: 150,
        }
    }
}

/// A mock memory lookup executor that simulates in-memory orderbook operations.
///
/// Designed to be held persistently by a worker — never recreated per call.
#[derive(Debug, Clone)]
pub struct MockMemoryLookup {
    config: MockMemoryLookupConfig,
}

impl MockMemoryLookup {
    /// Creates a new `MockMemoryLookup` with default configuration (50–150μs jitter).
    pub fn new() -> Self {
        Self {
            config: MockMemoryLookupConfig::default(),
        }
    }

    /// Samples a random delay in microseconds according to the configured distribution.
    pub fn sample_delay_us(&self) -> u64 {
        let mut rng = rand::thread_rng();
        rng.gen_range(self.config.min_delay_us..=self.config.max_delay_us)
    }

    /// Executes a mock memory lookup operation.
    pub async fn execute(&self, operation: &str, args: &[(String, String)]) -> Result<String, String> {
        let delay = self.sample_delay_us();
        sleep(Duration::from_micros(delay)).await;

        match operation {
            "lookup_orderbook" => {
                let symbol = args
                    .iter()
                    .find(|(k, _)| k == "symbol")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("UNKNOWN");
                Ok(format!(
                    r#"{{"symbol":"{}","bids":[[60000.0,1.5]],"asks":[[60001.0,2.0]]}}"#,
                    symbol
                ))
            }
            "check_risk_limit" => {
                let account_id = args
                    .iter()
                    .find(|(k, _)| k == "account_id")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("UNKNOWN");
                Ok(format!(
                    r#"{{"account_id":"{}","risk_ok":true,"max_drawdown":0.02}}"#,
                    account_id
                ))
            }
            "write_trade_record" => {
                Ok(r#"{"trade_id":"TRD-1001","status":"recorded"}"#.to_string())
            }
            _ => Err(format!("unknown memory lookup operation: {}", operation)),
        }
    }
}

impl Default for MockMemoryLookup {
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
        let tool = MockMemoryLookup::new();
        // Assert sampled delays fall strictly within the 50–150μs range per §4.5
        for _ in 0..100 {
            let delay = tool.sample_delay_us();
            assert!(delay >= 50, "expected delay >= 50us, got {}us", delay);
            assert!(delay <= 150, "expected delay <= 150us, got {}us", delay);
        }
    }

    #[tokio::test]
    async fn test_execution_latency() {
        let tool = MockMemoryLookup::new();
        let start = Instant::now();
        let _ = tool.execute("lookup_orderbook", &[("symbol".to_string(), "BTC-USD".to_string())]).await.unwrap();
        let elapsed_us = start.elapsed().as_micros() as u64;
        // Allow buffer for OS scheduler / Windows 15.6ms timer resolution in test environments
        assert!(elapsed_us < 50_000, "expected execution under 50ms, got {}us", elapsed_us);
    }

    #[tokio::test]
    async fn test_operations() {
        let tool = MockMemoryLookup::new();
        let res = tool.execute("check_risk_limit", &[("account_id".to_string(), "ACC-1".to_string())]).await.unwrap();
        assert!(res.contains("risk_ok"));
        let res2 = tool.execute("write_trade_record", &[]).await.unwrap();
        assert!(res2.contains("recorded"));
    }
}
