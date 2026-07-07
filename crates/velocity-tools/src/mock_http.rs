//! Mock HTTP API tool executor.
//!
//! Simulates an external API call with a configurable delay (default 20–50ms
//! jittered) representing a third-party pricing/inventory API.
//!
//! The delay distribution must match identically across all benchmark contenders.

use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for the mock HTTP executor.
#[derive(Debug, Clone)]
pub struct MockHttpConfig {
    /// Minimum simulated API latency in microseconds.
    pub min_delay_us: u64,
    /// Maximum simulated API latency in microseconds.
    pub max_delay_us: u64,
}

impl Default for MockHttpConfig {
    fn default() -> Self {
        Self {
            min_delay_us: 20_000,
            max_delay_us: 50_000,
        }
    }
}

impl MockHttpConfig {
    /// Sub-millisecond HFT profile configuration (200–500μs).
    pub fn hft() -> Self {
        Self {
            min_delay_us: 200,
            max_delay_us: 500,
        }
    }
}

/// A mock HTTP API executor that simulates external service calls.
///
/// Designed to be held persistently by a worker — never recreated per call.
#[derive(Debug, Clone)]
pub struct MockHttp {
    config: MockHttpConfig,
}

impl MockHttp {
    /// Creates a new `MockHttp` with the given configuration.
    pub fn new(config: MockHttpConfig) -> Self {
        Self { config }
    }

    /// Creates a new `MockHttp` with default configuration (20–50ms jitter).
    pub fn with_defaults() -> Self {
        Self::new(MockHttpConfig::default())
    }

    /// Creates a new `MockHttp` with HFT configuration (200–500μs jitter).
    pub fn with_hft() -> Self {
        Self::new(MockHttpConfig::hft())
    }

    /// Executes a mock HTTP API operation.
    ///
    /// Simulates API latency with a uniformly distributed random delay,
    /// then returns a fixed-schema response.
    pub async fn execute(&self, operation: &str, args: &[(String, String)]) -> Result<String, String> {
        // Simulate jittered API latency
        let delay = {
            let mut rng = rand::thread_rng();
            rng.gen_range(self.config.min_delay_us..=self.config.max_delay_us)
        };
        sleep(Duration::from_micros(delay)).await;

        match operation {
            "get_pricing" => {
                let sku = args
                    .iter()
                    .find(|(k, _)| k == "sku")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("UNKNOWN");
                Ok(format!(
                    r#"{{"sku":"{}","unit_price":29.99,"currency":"USD","available":true}}"#,
                    sku
                ))
            }
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
            _ => Err(format!("unknown http operation: {}", operation)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_pricing() {
        let http = MockHttp::with_defaults();
        let args = vec![("sku".to_string(), "WIDGET-001".to_string())];
        let result = http.execute("get_pricing", &args).await;
        assert!(result.is_ok());
        let payload = result.unwrap();
        assert!(payload.contains("WIDGET-001"));
        assert!(payload.contains("unit_price"));
    }

    #[tokio::test]
    async fn test_calculate_alpha() {
        let http = MockHttp::with_hft();
        let args = vec![("symbol".to_string(), "BTC-USD".to_string())];
        let result = http.execute("calculate_alpha", &args).await;
        assert!(result.is_ok());
        let payload = result.unwrap();
        assert!(payload.contains("BTC-USD"));
        assert!(payload.contains("alpha_score"));
    }

    #[tokio::test]
    async fn test_unknown_operation() {
        let http = MockHttp::with_defaults();
        let result = http.execute("delete_everything", &[]).await;
        assert!(result.is_err());
    }
}
