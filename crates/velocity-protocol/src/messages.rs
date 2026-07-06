//! Wire protocol message types for the Velocity runtime.
//!
//! All messages share a common 8-byte header:
//! ```text
//! +--------------+--------------+--------------+-------------------+
//! |  magic (2B)  | version (1B) | msg_type(1B) | payload_len (4B)  |
//! +--------------+--------------+--------------+-------------------+
//! |                     payload (variable)                         |
//! +---------------------------------------------------------------+
//! ```
//!
//! Magic bytes: `0x56 0x4C` ("VL")
//! All integers are little-endian.
//! Variable-length strings are prefixed with a u32 length.

/// Magic bytes for framing validation: "VL" (Velocity).
pub const MAGIC: [u8; 2] = [0x56, 0x4C];

/// Current protocol version.
pub const VERSION: u8 = 1;

/// Fixed header size in bytes: magic(2) + version(1) + msg_type(1) + payload_len(4).
pub const HEADER_SIZE: usize = 8;

/// Message type discriminant encoded in the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// A tool call request from the scheduler to a worker.
    ToolCallRequest = 0x01,
    /// A tool call response from a worker back to the scheduler.
    ToolCallResponse = 0x02,
    /// A lightweight heartbeat for worker liveness detection.
    Heartbeat = 0x03,
    /// An error message.
    Error = 0x04,
}

impl MessageType {
    /// Converts a raw byte to a `MessageType`, returning `None` for unknown values.
    ///
    /// This performs no heap allocation and is a simple match.
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::ToolCallRequest),
            0x02 => Some(Self::ToolCallResponse),
            0x03 => Some(Self::Heartbeat),
            0x04 => Some(Self::Error),
            _ => None,
        }
    }
}

/// A request to execute a tool call.
///
/// Contains the tool name, operation, and a list of key-value argument pairs.
/// All strings are variable-length with u32 length prefixes in the wire format.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallRequest {
    /// Unique identifier for this request, used for correlation with responses.
    pub request_id: u64,
    /// Name of the tool to invoke (e.g., "mock_db", "mock_http", "mock_file").
    pub tool_name: String,
    /// The specific operation within the tool (e.g., "lookup_account", "get_pricing").
    pub operation: String,
    /// Key-value argument pairs passed to the tool operation.
    pub args: Vec<(String, String)>,
}

/// A response from a completed tool call.
///
/// Contains the correlation request ID, success/failure status, and payload data.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallResponse {
    /// The request ID this response correlates to.
    pub request_id: u64,
    /// Whether the tool call succeeded.
    pub success: bool,
    /// The response payload (result data on success, error detail on failure).
    pub payload: String,
    /// Execution time in microseconds, for instrumentation.
    pub execution_time_us: u64,
}

/// A lightweight heartbeat message for worker liveness detection.
///
/// Workers send these periodically so the pool can detect and replace dead workers
/// proactively, never lazily on the request path.
#[derive(Debug, Clone, PartialEq)]
pub struct Heartbeat {
    /// Identifier of the worker sending the heartbeat.
    pub worker_id: u64,
    /// Monotonic timestamp in microseconds.
    pub timestamp_us: u64,
}

/// An error message sent over the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorMessage {
    /// The request ID this error relates to (0 if not request-specific).
    pub request_id: u64,
    /// Numeric error code.
    pub error_code: u32,
    /// Human-readable error description.
    pub message: String,
}
