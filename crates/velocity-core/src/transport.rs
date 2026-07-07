//! Transport layer for encoding/decoding tool calls over the binary wire protocol.
//!
//! This module bridges the scheduler and worker pool with the binary codec,
//! handling the encode → dispatch → decode pipeline for each tool call.

use std::time::Instant;
use velocity_protocol::{
    decode_response, decode_tool_call, encode_response, encode_tool_call, ToolCallRequest,
    ToolCallResponse,
};

/// Timestamps captured at each stage of a tool call for instrumentation.
///
/// Every hop that could plausibly show up as latency in the benchmark emits
/// a timestamp via these fields. Retrofitting instrumentation after the fact
/// produces untrustworthy numbers, so we capture eagerly.
#[derive(Debug, Clone)]
pub struct CallTrace {
    /// When the request was received by the runtime.
    pub request_received: Instant,
    /// When the request was encoded to binary.
    pub request_encoded: Option<Instant>,
    /// When a worker was acquired from the pool.
    pub worker_acquired: Option<Instant>,
    /// When the tool execution completed.
    pub tool_executed: Option<Instant>,
    /// When the response was encoded to binary.
    pub response_encoded: Option<Instant>,
    /// When the response was returned to the caller.
    pub response_returned: Option<Instant>,
}

impl CallTrace {
    /// Creates a new trace with the current timestamp as `request_received`.
    pub fn new() -> Self {
        Self {
            request_received: Instant::now(),
            request_encoded: None,
            worker_acquired: None,
            tool_executed: None,
            response_encoded: None,
            response_returned: None,
        }
    }

    /// Total elapsed time from request received to response returned, in microseconds.
    ///
    /// Returns `None` if the trace is incomplete.
    pub fn total_us(&self) -> Option<u64> {
        self.response_returned
            .map(|t| t.duration_since(self.request_received).as_micros() as u64)
    }

    /// Time spent encoding the request, in microseconds.
    pub fn encode_us(&self) -> Option<u64> {
        self.request_encoded
            .map(|t| t.duration_since(self.request_received).as_micros() as u64)
    }

    /// Time spent acquiring a worker, in microseconds.
    pub fn acquire_us(&self) -> Option<u64> {
        match (self.request_encoded, self.worker_acquired) {
            (Some(enc), Some(acq)) => Some(acq.duration_since(enc).as_micros() as u64),
            _ => None,
        }
    }

    /// Time spent executing the tool, in microseconds.
    pub fn execute_us(&self) -> Option<u64> {
        match (self.worker_acquired, self.tool_executed) {
            (Some(acq), Some(exec)) => Some(exec.duration_since(acq).as_micros() as u64),
            _ => None,
        }
    }
}

impl Default for CallTrace {
    fn default() -> Self {
        Self::new()
    }
}

/// Encodes a tool call request through the binary wire protocol.
///
/// This function performs a single allocation for the output buffer.
/// Returns the encoded bytes and updates the trace with the encode timestamp.
pub fn encode_request(req: &ToolCallRequest, trace: &mut CallTrace) -> Vec<u8> {
    let bytes = encode_tool_call(req);
    trace.request_encoded = Some(Instant::now());
    bytes
}

/// Decodes a tool call request from binary wire bytes.
///
/// Used by workers receiving dispatched requests. Zero-copy where possible.
pub fn decode_request(bytes: &[u8]) -> Result<ToolCallRequest, velocity_protocol::ProtocolError> {
    decode_tool_call(bytes)
}

/// Encodes a tool call response through the binary wire protocol.
///
/// Updates the trace with the encode timestamp.
pub fn encode_tool_response(resp: &ToolCallResponse, trace: &mut CallTrace) -> Vec<u8> {
    let bytes = encode_response(resp);
    trace.response_encoded = Some(Instant::now());
    bytes
}

/// Decodes a tool call response from binary wire bytes.
pub fn decode_tool_response(
    bytes: &[u8],
) -> Result<ToolCallResponse, velocity_protocol::ProtocolError> {
    decode_response(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_request_through_transport() {
        let req = ToolCallRequest {
            request_id: 42,
            tool_name: "mock_db".to_string(),
            operation: "lookup_account".to_string(),
            args: vec![("account_id".to_string(), "ACC-123".to_string())],
        };

        let mut trace = CallTrace::new();
        let encoded = encode_request(&req, &mut trace);
        assert!(trace.request_encoded.is_some());

        let decoded = decode_request(&encoded).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_encode_decode_response_through_transport() {
        let resp = ToolCallResponse {
            request_id: 42,
            success: true,
            payload: "result data".to_string(),
            execution_time_us: 5000,
        };

        let mut trace = CallTrace::new();
        let encoded = encode_tool_response(&resp, &mut trace);
        assert!(trace.response_encoded.is_some());

        let decoded = decode_tool_response(&encoded).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_call_trace_timing() {
        let mut trace = CallTrace::new();
        assert!(trace.total_us().is_none());

        trace.request_encoded = Some(Instant::now());
        trace.worker_acquired = Some(Instant::now());
        trace.tool_executed = Some(Instant::now());
        trace.response_encoded = Some(Instant::now());
        trace.response_returned = Some(Instant::now());

        assert!(trace.total_us().is_some());
        assert!(trace.encode_us().is_some());
        assert!(trace.acquire_us().is_some());
        assert!(trace.execute_us().is_some());
    }
}
