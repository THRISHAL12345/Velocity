//! Mock tool executors for the Velocity runtime benchmarks.
//!
//! Three tools with deliberately simple logic but realistic latency shapes:
//! - `mock_db`: 5–15ms jittered (simulates database queries)
//! - `mock_http`: 20–50ms jittered (simulates external API calls)
//! - `mock_file`: 1–3ms jittered (simulates local file I/O)
//!
//! All three are implemented identically across the Velocity runtime,
//! LangGraph baseline, and raw MCP baseline.

pub mod mock_db;
pub mod mock_file;
pub mod mock_http;

pub use mock_db::{MockDb, MockDbConfig};
pub use mock_file::{MockFile, MockFileConfig};
pub use mock_http::{MockHttp, MockHttpConfig};
