//! Mock database tool executor.
//!
//! Simulates a database query with a configurable artificial delay (default 5–15ms
//! jittered) to mimic real query latency. Returns a small fixed-schema record.
//!
//! The delay distribution must match identically across the Velocity runtime,
//! LangGraph baseline, and raw MCP baseline so the benchmark isolates
//! orchestration overhead, not tool-implementation differences.

use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for the mock database executor.
#[derive(Debug, Clone)]
pub struct MockDbConfig {
    /// Minimum simulated query latency in microseconds.
    pub min_delay_us: u64,
    /// Maximum simulated query latency in microseconds.
    pub max_delay_us: u64,
}

impl Default for MockDbConfig {
    fn default() -> Self {
        Self {
            min_delay_us: 5_000,
            max_delay_us: 15_000,
        }
    }
}

impl MockDbConfig {
    /// Sub-millisecond HFT profile configuration (50–500μs).
    pub fn hft() -> Self {
        Self {
            min_delay_us: 50,
            max_delay_us: 500,
        }
    }
}

/// A mock database executor that holds its configuration and simulates queries.
///
/// Designed to be held persistently by a worker — never recreated per call.
#[derive(Debug, Clone)]
pub struct MockDb {
    config: MockDbConfig,
}

impl MockDb {
    /// Creates a new `MockDb` with the given configuration.
    pub fn new(config: MockDbConfig) -> Self {
        Self { config }
    }

    /// Creates a new `MockDb` with default configuration (5–15ms jitter).
    pub fn with_defaults() -> Self {
        Self::new(MockDbConfig::default())
    }

    /// Creates a new `MockDb` with HFT configuration (50–500μs jitter).
    pub fn with_hft() -> Self {
        Self::new(MockDbConfig::hft())
    }

    /// Executes a mock database operation.
    ///
    /// Simulates query latency with a uniformly distributed random delay
    /// between `min_delay_us` and `max_delay_us`, then returns a fixed-schema
    /// response based on the operation name.
    pub async fn execute(
        &self,
        operation: &str,
        args: &[(String, String)],
    ) -> Result<String, String> {
        // Simulate jittered query latency
        let delay = {
            let mut rng = rand::thread_rng();
            rng.gen_range(self.config.min_delay_us..=self.config.max_delay_us)
        };
        sleep(Duration::from_micros(delay)).await;

        match operation {
            "lookup_account" => {
                let account_id = args
                    .iter()
                    .find(|(k, _)| k == "account_id")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("UNKNOWN");
                Ok(format!(
                    r#"{{"account_id":"{}","name":"Test User","balance":1000.50,"status":"active"}}"#,
                    account_id
                ))
            }
            "check_inventory" => {
                let sku = args
                    .iter()
                    .find(|(k, _)| k == "sku")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("UNKNOWN");
                Ok(format!(
                    r#"{{"sku":"{}","quantity":42,"warehouse":"WH-001"}}"#,
                    sku
                ))
            }
            "write_order_record" => {
                Ok(r#"{"order_id":"ORD-99001","status":"confirmed"}"#.to_string())
            }
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
            _ => Err(format!("unknown db operation: {}", operation)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_lookup_account() {
        let db = MockDb::with_defaults();
        let args = vec![("account_id".to_string(), "ACC-123".to_string())];
        let result = db.execute("lookup_account", &args).await;
        assert!(result.is_ok());
        let payload = result.unwrap();
        assert!(payload.contains("ACC-123"));
        assert!(payload.contains("balance"));
    }

    #[tokio::test]
    async fn test_check_inventory() {
        let db = MockDb::with_defaults();
        let args = vec![("sku".to_string(), "WIDGET-001".to_string())];
        let result = db.execute("check_inventory", &args).await;
        assert!(result.is_ok());
        let payload = result.unwrap();
        assert!(payload.contains("WIDGET-001"));
        assert!(payload.contains("quantity"));
    }

    #[tokio::test]
    async fn test_write_order_record() {
        let db = MockDb::with_defaults();
        let result = db.execute("write_order_record", &[]).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("confirmed"));
    }

    #[tokio::test]
    async fn test_hft_operations() {
        let db = MockDb::with_hft();
        let args = vec![("symbol".to_string(), "BTC-USD".to_string())];
        let res = db.execute("lookup_orderbook", &args).await.unwrap();
        assert!(res.contains("BTC-USD"));

        let args2 = vec![("account_id".to_string(), "TRADER-01".to_string())];
        let res2 = db.execute("check_risk_limit", &args2).await.unwrap();
        assert!(res2.contains("risk_ok"));

        let res3 = db.execute("write_trade_record", &[]).await.unwrap();
        assert!(res3.contains("recorded"));
    }

    #[tokio::test]
    async fn test_unknown_operation() {
        let db = MockDb::with_defaults();
        let result = db.execute("drop_tables", &[]).await;
        assert!(result.is_err());
    }
}
