//! Mock file I/O tool executor.
//!
//! Simulates file read/write operations with minimal delay (default 1–3ms),
//! representing local I/O.
//!
//! The delay distribution must match identically across all benchmark contenders.

use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;

/// Configuration for the mock file executor.
#[derive(Debug, Clone)]
pub struct MockFileConfig {
    /// Minimum simulated file I/O latency in microseconds.
    pub min_delay_us: u64,
    /// Maximum simulated file I/O latency in microseconds.
    pub max_delay_us: u64,
}

impl Default for MockFileConfig {
    fn default() -> Self {
        Self {
            min_delay_us: 1_000,
            max_delay_us: 3_000,
        }
    }
}

impl MockFileConfig {
    /// Sub-millisecond HFT profile configuration (50–150μs).
    pub fn hft() -> Self {
        Self {
            min_delay_us: 50,
            max_delay_us: 150,
        }
    }
}

/// A mock file I/O executor that simulates local file operations.
///
/// Designed to be held persistently by a worker — never recreated per call.
#[derive(Debug, Clone)]
pub struct MockFile {
    config: MockFileConfig,
}

impl MockFile {
    /// Creates a new `MockFile` with the given configuration.
    pub fn new(config: MockFileConfig) -> Self {
        Self { config }
    }

    /// Creates a new `MockFile` with default configuration (1–3ms jitter).
    pub fn with_defaults() -> Self {
        Self::new(MockFileConfig::default())
    }

    /// Creates a new `MockFile` with HFT configuration (50–150μs jitter).
    pub fn with_hft() -> Self {
        Self::new(MockFileConfig::hft())
    }

    /// Executes a mock file I/O operation.
    ///
    /// Simulates file I/O latency with a uniformly distributed random delay,
    /// then returns a fixed-schema response.
    pub async fn execute(
        &self,
        operation: &str,
        args: &[(String, String)],
    ) -> Result<String, String> {
        // Simulate jittered file I/O latency
        let delay = {
            let mut rng = rand::thread_rng();
            rng.gen_range(self.config.min_delay_us..=self.config.max_delay_us)
        };
        sleep(Duration::from_micros(delay)).await;

        match operation {
            "write_confirmation_log" => {
                let order_id = args
                    .iter()
                    .find(|(k, _)| k == "order_id")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("UNKNOWN");
                Ok(format!(
                    r#"{{"file":"/var/log/orders/{}.log","bytes_written":256,"status":"ok"}}"#,
                    order_id
                ))
            }
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
            "read" => Ok(r#"{"content":"file contents here","bytes_read":128}"#.to_string()),
            _ => Err(format!("unknown file operation: {}", operation)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_confirmation_log() {
        let file = MockFile::with_defaults();
        let args = vec![("order_id".to_string(), "ORD-99001".to_string())];
        let result = file.execute("write_confirmation_log", &args).await;
        assert!(result.is_ok());
        let payload = result.unwrap();
        assert!(payload.contains("ORD-99001"));
        assert!(payload.contains("bytes_written"));
    }

    #[tokio::test]
    async fn test_read() {
        let file = MockFile::with_defaults();
        let result = file.execute("read", &[]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_log_audit() {
        let file = MockFile::with_hft();
        let args = vec![("trade_id".to_string(), "TRD-1001".to_string())];
        let result = file.execute("log_audit", &args).await;
        assert!(result.is_ok());
        let payload = result.unwrap();
        assert!(payload.contains("TRD-1001"));
        assert!(payload.contains("bytes_written"));
    }

    #[tokio::test]
    async fn test_unknown_operation() {
        let file = MockFile::with_defaults();
        let result = file.execute("format_drive", &[]).await;
        assert!(result.is_err());
    }
}
