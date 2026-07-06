//! Binary wire protocol codec for the Velocity runtime.
//!
//! Hand-rolled struct packing with zero reflection and compile-time known schemas.
//! All encode functions produce a complete wire message (header + payload).
//! All decode functions consume raw bytes and return structured messages.
//!
//! Performance target: encode+decode round trip under 5 microseconds.

use crate::messages::*;
use thiserror::Error;

/// Errors that can occur during protocol encoding/decoding.
#[derive(Debug, Error, PartialEq)]
pub enum ProtocolError {
    /// The message buffer is too short to contain a valid header.
    #[error("buffer too short: expected at least {expected} bytes, got {actual}")]
    BufferTooShort { expected: usize, actual: usize },

    /// The magic bytes do not match the expected VL prefix.
    #[error("invalid magic bytes: expected 0x564C, got 0x{0:02X}{1:02X}")]
    InvalidMagic(u8, u8),

    /// The protocol version is not supported.
    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(u8),

    /// The message type byte is not recognized.
    #[error("unknown message type: 0x{0:02X}")]
    UnknownMessageType(u8),

    /// The payload length exceeds the maximum allowed size (16 MiB).
    #[error("payload too large: {size} bytes exceeds maximum of {max} bytes")]
    PayloadTooLarge { size: u32, max: u32 },

    /// The payload is truncated or malformed.
    #[error("malformed payload: {0}")]
    MalformedPayload(String),
}

/// Maximum allowed payload size: 16 MiB. Prevents allocation bombs.
const MAX_PAYLOAD_SIZE: u32 = 16 * 1024 * 1024;

// ─── Encoding ────────────────────────────────────────────────────────────────

/// Encodes a `ToolCallRequest` into a complete wire message.
///
/// Wire layout (payload):
/// ```text
/// request_id (u64) | tool_name_len (u32) | tool_name (bytes)
/// | operation_len (u32) | operation (bytes)
/// | args_count (u32) | [key_len (u32) | key (bytes) | val_len (u32) | val (bytes)] ...
/// ```
///
/// This function allocates a single `Vec<u8>` sized to the exact payload length,
/// avoiding reallocation on the fast path for typical payloads.
pub fn encode_tool_call(req: &ToolCallRequest) -> Vec<u8> {
    let payload_size = 8 // request_id
        + 4 + req.tool_name.len()
        + 4 + req.operation.len()
        + 4 // args_count
        + req.args.iter().map(|(k, v)| 4 + k.len() + 4 + v.len()).sum::<usize>();

    let total = HEADER_SIZE + payload_size;
    let mut buf = Vec::with_capacity(total);

    // Header
    write_header(&mut buf, MessageType::ToolCallRequest, payload_size as u32);

    // Payload
    buf.extend_from_slice(&req.request_id.to_le_bytes());
    write_string(&mut buf, &req.tool_name);
    write_string(&mut buf, &req.operation);
    buf.extend_from_slice(&(req.args.len() as u32).to_le_bytes());
    for (key, val) in &req.args {
        write_string(&mut buf, key);
        write_string(&mut buf, val);
    }

    buf
}

/// Decodes a `ToolCallRequest` from raw wire bytes.
///
/// Validates the header (magic, version, message type) before parsing the payload.
/// Returns an error for any malformed input without panicking.
pub fn decode_tool_call(bytes: &[u8]) -> Result<ToolCallRequest, ProtocolError> {
    let (msg_type, payload) = decode_header(bytes)?;
    if msg_type != MessageType::ToolCallRequest {
        return Err(ProtocolError::UnknownMessageType(msg_type as u8));
    }

    let mut offset = 0;

    let request_id = read_u64(payload, &mut offset)?;
    let tool_name = read_string(payload, &mut offset)?;
    let operation = read_string(payload, &mut offset)?;
    let args_count = read_u32(payload, &mut offset)? as usize;

    let mut args = Vec::with_capacity(args_count);
    for _ in 0..args_count {
        let key = read_string(payload, &mut offset)?;
        let val = read_string(payload, &mut offset)?;
        args.push((key, val));
    }

    Ok(ToolCallRequest {
        request_id,
        tool_name,
        operation,
        args,
    })
}

/// Encodes a `ToolCallResponse` into a complete wire message.
///
/// Wire layout (payload):
/// ```text
/// request_id (u64) | success (u8) | payload_len (u32) | payload (bytes)
/// | execution_time_us (u64)
/// ```
pub fn encode_response(resp: &ToolCallResponse) -> Vec<u8> {
    let payload_size = 8 // request_id
        + 1 // success flag
        + 4 + resp.payload.len()
        + 8; // execution_time_us

    let total = HEADER_SIZE + payload_size;
    let mut buf = Vec::with_capacity(total);

    // Header
    write_header(&mut buf, MessageType::ToolCallResponse, payload_size as u32);

    // Payload
    buf.extend_from_slice(&resp.request_id.to_le_bytes());
    buf.push(if resp.success { 1 } else { 0 });
    write_string(&mut buf, &resp.payload);
    buf.extend_from_slice(&resp.execution_time_us.to_le_bytes());

    buf
}

/// Decodes a `ToolCallResponse` from raw wire bytes.
///
/// Validates the header before parsing. Returns an error for malformed input.
pub fn decode_response(bytes: &[u8]) -> Result<ToolCallResponse, ProtocolError> {
    let (msg_type, payload) = decode_header(bytes)?;
    if msg_type != MessageType::ToolCallResponse {
        return Err(ProtocolError::UnknownMessageType(msg_type as u8));
    }

    let mut offset = 0;

    let request_id = read_u64(payload, &mut offset)?;
    let success = read_u8(payload, &mut offset)? != 0;
    let payload_str = read_string(payload, &mut offset)?;
    let execution_time_us = read_u64(payload, &mut offset)?;

    Ok(ToolCallResponse {
        request_id,
        success,
        payload: payload_str,
        execution_time_us,
    })
}

/// Encodes a `Heartbeat` into a complete wire message.
///
/// Wire layout (payload): `worker_id (u64) | timestamp_us (u64)`
///
/// This is the smallest possible message — 16-byte payload, zero strings.
pub fn encode_heartbeat(hb: &Heartbeat) -> Vec<u8> {
    let payload_size = 16; // two u64s
    let mut buf = Vec::with_capacity(HEADER_SIZE + payload_size);

    write_header(&mut buf, MessageType::Heartbeat, payload_size as u32);
    buf.extend_from_slice(&hb.worker_id.to_le_bytes());
    buf.extend_from_slice(&hb.timestamp_us.to_le_bytes());

    buf
}

/// Decodes a `Heartbeat` from raw wire bytes.
pub fn decode_heartbeat(bytes: &[u8]) -> Result<Heartbeat, ProtocolError> {
    let (msg_type, payload) = decode_header(bytes)?;
    if msg_type != MessageType::Heartbeat {
        return Err(ProtocolError::UnknownMessageType(msg_type as u8));
    }

    let mut offset = 0;
    let worker_id = read_u64(payload, &mut offset)?;
    let timestamp_us = read_u64(payload, &mut offset)?;

    Ok(Heartbeat {
        worker_id,
        timestamp_us,
    })
}

/// Encodes an `ErrorMessage` into a complete wire message.
///
/// Wire layout (payload):
/// `request_id (u64) | error_code (u32) | message_len (u32) | message (bytes)`
pub fn encode_error(err: &ErrorMessage) -> Vec<u8> {
    let payload_size = 8 + 4 + 4 + err.message.len();
    let mut buf = Vec::with_capacity(HEADER_SIZE + payload_size);

    write_header(&mut buf, MessageType::Error, payload_size as u32);
    buf.extend_from_slice(&err.request_id.to_le_bytes());
    buf.extend_from_slice(&err.error_code.to_le_bytes());
    write_string(&mut buf, &err.message);

    buf
}

/// Decodes an `ErrorMessage` from raw wire bytes.
pub fn decode_error(bytes: &[u8]) -> Result<ErrorMessage, ProtocolError> {
    let (msg_type, payload) = decode_header(bytes)?;
    if msg_type != MessageType::Error {
        return Err(ProtocolError::UnknownMessageType(msg_type as u8));
    }

    let mut offset = 0;
    let request_id = read_u64(payload, &mut offset)?;
    let error_code = read_u32(payload, &mut offset)?;
    let message = read_string(payload, &mut offset)?;

    Ok(ErrorMessage {
        request_id,
        error_code,
        message,
    })
}

// ─── Internal helpers ────────────────────────────────────────────────────────

/// Writes the 8-byte header to the buffer.
fn write_header(buf: &mut Vec<u8>, msg_type: MessageType, payload_len: u32) {
    buf.extend_from_slice(&MAGIC);
    buf.push(VERSION);
    buf.push(msg_type as u8);
    buf.extend_from_slice(&payload_len.to_le_bytes());
}

/// Writes a length-prefixed string (u32 length + raw bytes).
fn write_string(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

/// Validates and strips the header, returning the message type and payload slice.
fn decode_header(bytes: &[u8]) -> Result<(MessageType, &[u8]), ProtocolError> {
    if bytes.len() < HEADER_SIZE {
        return Err(ProtocolError::BufferTooShort {
            expected: HEADER_SIZE,
            actual: bytes.len(),
        });
    }

    // Validate magic
    if bytes[0] != MAGIC[0] || bytes[1] != MAGIC[1] {
        return Err(ProtocolError::InvalidMagic(bytes[0], bytes[1]));
    }

    // Validate version
    if bytes[2] != VERSION {
        return Err(ProtocolError::UnsupportedVersion(bytes[2]));
    }

    // Parse message type
    let msg_type = MessageType::from_u8(bytes[3])
        .ok_or(ProtocolError::UnknownMessageType(bytes[3]))?;

    // Parse and validate payload length
    let payload_len = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if payload_len > MAX_PAYLOAD_SIZE {
        return Err(ProtocolError::PayloadTooLarge {
            size: payload_len,
            max: MAX_PAYLOAD_SIZE,
        });
    }

    let total_expected = HEADER_SIZE + payload_len as usize;
    if bytes.len() < total_expected {
        return Err(ProtocolError::BufferTooShort {
            expected: total_expected,
            actual: bytes.len(),
        });
    }

    Ok((msg_type, &bytes[HEADER_SIZE..total_expected]))
}

/// Reads a u8 from the payload at the given offset, advancing the offset.
fn read_u8(payload: &[u8], offset: &mut usize) -> Result<u8, ProtocolError> {
    if *offset + 1 > payload.len() {
        return Err(ProtocolError::MalformedPayload(
            format!("expected 1 byte at offset {}, but payload is {} bytes", offset, payload.len()),
        ));
    }
    let val = payload[*offset];
    *offset += 1;
    Ok(val)
}

/// Reads a little-endian u32 from the payload at the given offset, advancing the offset.
fn read_u32(payload: &[u8], offset: &mut usize) -> Result<u32, ProtocolError> {
    if *offset + 4 > payload.len() {
        return Err(ProtocolError::MalformedPayload(
            format!("expected 4 bytes at offset {}, but payload is {} bytes", offset, payload.len()),
        ));
    }
    let val = u32::from_le_bytes([
        payload[*offset],
        payload[*offset + 1],
        payload[*offset + 2],
        payload[*offset + 3],
    ]);
    *offset += 4;
    Ok(val)
}

/// Reads a little-endian u64 from the payload at the given offset, advancing the offset.
fn read_u64(payload: &[u8], offset: &mut usize) -> Result<u64, ProtocolError> {
    if *offset + 8 > payload.len() {
        return Err(ProtocolError::MalformedPayload(
            format!("expected 8 bytes at offset {}, but payload is {} bytes", offset, payload.len()),
        ));
    }
    let val = u64::from_le_bytes([
        payload[*offset],
        payload[*offset + 1],
        payload[*offset + 2],
        payload[*offset + 3],
        payload[*offset + 4],
        payload[*offset + 5],
        payload[*offset + 6],
        payload[*offset + 7],
    ]);
    *offset += 8;
    Ok(val)
}

/// Reads a length-prefixed string from the payload at the given offset.
fn read_string(payload: &[u8], offset: &mut usize) -> Result<String, ProtocolError> {
    let len = read_u32(payload, offset)? as usize;
    if *offset + len > payload.len() {
        return Err(ProtocolError::MalformedPayload(
            format!("string of length {} at offset {} exceeds payload of {} bytes", len, offset, payload.len()),
        ));
    }
    let s = std::str::from_utf8(&payload[*offset..*offset + len])
        .map_err(|e| ProtocolError::MalformedPayload(format!("invalid UTF-8: {}", e)))?;
    *offset += len;
    Ok(s.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── ToolCallRequest round-trip ──────────────────────────────────────────

    #[test]
    fn test_tool_call_round_trip_basic() {
        let req = ToolCallRequest {
            request_id: 42,
            tool_name: "mock_db".to_string(),
            operation: "lookup_account".to_string(),
            args: vec![("account_id".to_string(), "ACC-12345".to_string())],
        };
        let encoded = encode_tool_call(&req);
        let decoded = decode_tool_call(&encoded).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_tool_call_round_trip_no_args() {
        let req = ToolCallRequest {
            request_id: 0,
            tool_name: "mock_file".to_string(),
            operation: "read".to_string(),
            args: vec![],
        };
        let encoded = encode_tool_call(&req);
        let decoded = decode_tool_call(&encoded).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_tool_call_round_trip_many_args() {
        let req = ToolCallRequest {
            request_id: u64::MAX,
            tool_name: "mock_http".to_string(),
            operation: "get_pricing".to_string(),
            args: vec![
                ("sku".to_string(), "WIDGET-001".to_string()),
                ("currency".to_string(), "USD".to_string()),
                ("quantity".to_string(), "100".to_string()),
            ],
        };
        let encoded = encode_tool_call(&req);
        let decoded = decode_tool_call(&encoded).unwrap();
        assert_eq!(req, decoded);
    }

    // ─── ToolCallResponse round-trip ─────────────────────────────────────────

    #[test]
    fn test_response_round_trip_success() {
        let resp = ToolCallResponse {
            request_id: 42,
            success: true,
            payload: r#"{"account":"ACC-12345","balance":1000.50}"#.to_string(),
            execution_time_us: 7500,
        };
        let encoded = encode_response(&resp);
        let decoded = decode_response(&encoded).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_response_round_trip_failure() {
        let resp = ToolCallResponse {
            request_id: 99,
            success: false,
            payload: "connection refused".to_string(),
            execution_time_us: 0,
        };
        let encoded = encode_response(&resp);
        let decoded = decode_response(&encoded).unwrap();
        assert_eq!(resp, decoded);
    }

    // ─── Heartbeat round-trip ────────────────────────────────────────────────

    #[test]
    fn test_heartbeat_round_trip() {
        let hb = Heartbeat {
            worker_id: 7,
            timestamp_us: 1_625_000_000_000,
        };
        let encoded = encode_heartbeat(&hb);
        let decoded = decode_heartbeat(&encoded).unwrap();
        assert_eq!(hb, decoded);
    }

    // ─── ErrorMessage round-trip ─────────────────────────────────────────────

    #[test]
    fn test_error_round_trip() {
        let err = ErrorMessage {
            request_id: 42,
            error_code: 503,
            message: "service unavailable".to_string(),
        };
        let encoded = encode_error(&err);
        let decoded = decode_error(&encoded).unwrap();
        assert_eq!(err, decoded);
    }

    // ─── Edge cases: malformed input ─────────────────────────────────────────

    #[test]
    fn test_decode_empty_buffer() {
        let result = decode_tool_call(&[]);
        assert!(matches!(result, Err(ProtocolError::BufferTooShort { .. })));
    }

    #[test]
    fn test_decode_truncated_header() {
        let result = decode_tool_call(&[0x56, 0x4C, 0x01]);
        assert!(matches!(result, Err(ProtocolError::BufferTooShort { .. })));
    }

    #[test]
    fn test_decode_bad_magic() {
        let mut bytes = encode_tool_call(&ToolCallRequest {
            request_id: 1,
            tool_name: "t".to_string(),
            operation: "o".to_string(),
            args: vec![],
        });
        bytes[0] = 0xFF;
        bytes[1] = 0xFE;
        let result = decode_tool_call(&bytes);
        assert!(matches!(result, Err(ProtocolError::InvalidMagic(0xFF, 0xFE))));
    }

    #[test]
    fn test_decode_bad_version() {
        let mut bytes = encode_tool_call(&ToolCallRequest {
            request_id: 1,
            tool_name: "t".to_string(),
            operation: "o".to_string(),
            args: vec![],
        });
        bytes[2] = 99;
        let result = decode_tool_call(&bytes);
        assert!(matches!(result, Err(ProtocolError::UnsupportedVersion(99))));
    }

    #[test]
    fn test_decode_unknown_message_type() {
        let mut bytes = encode_tool_call(&ToolCallRequest {
            request_id: 1,
            tool_name: "t".to_string(),
            operation: "o".to_string(),
            args: vec![],
        });
        bytes[3] = 0xFF;
        let result = decode_tool_call(&bytes);
        assert!(matches!(result, Err(ProtocolError::UnknownMessageType(0xFF))));
    }

    #[test]
    fn test_decode_oversized_payload() {
        let mut bytes = vec![0x56, 0x4C, VERSION, 0x01];
        // payload_len = MAX_PAYLOAD_SIZE + 1
        let bad_len = MAX_PAYLOAD_SIZE + 1;
        bytes.extend_from_slice(&bad_len.to_le_bytes());
        let result = decode_tool_call(&bytes);
        assert!(matches!(result, Err(ProtocolError::PayloadTooLarge { .. })));
    }

    #[test]
    fn test_decode_truncated_payload() {
        let req = ToolCallRequest {
            request_id: 1,
            tool_name: "mock_db".to_string(),
            operation: "lookup".to_string(),
            args: vec![],
        };
        let encoded = encode_tool_call(&req);
        // Chop off the last 5 bytes of the payload
        let truncated = &encoded[..encoded.len() - 5];
        let result = decode_tool_call(truncated);
        assert!(matches!(result, Err(ProtocolError::BufferTooShort { .. })));
    }

    #[test]
    fn test_decode_wrong_message_type_for_response() {
        // Encode a request, try to decode as response
        let req = ToolCallRequest {
            request_id: 1,
            tool_name: "t".to_string(),
            operation: "o".to_string(),
            args: vec![],
        };
        let encoded = encode_tool_call(&req);
        let result = decode_response(&encoded);
        assert!(matches!(result, Err(ProtocolError::UnknownMessageType(_))));
    }

    // ─── Header validation ───────────────────────────────────────────────────

    #[test]
    fn test_header_magic_bytes() {
        let req = ToolCallRequest {
            request_id: 1,
            tool_name: "t".to_string(),
            operation: "o".to_string(),
            args: vec![],
        };
        let encoded = encode_tool_call(&req);
        assert_eq!(encoded[0], 0x56); // 'V'
        assert_eq!(encoded[1], 0x4C); // 'L'
        assert_eq!(encoded[2], VERSION);
        assert_eq!(encoded[3], MessageType::ToolCallRequest as u8);
    }

    #[test]
    fn test_empty_strings() {
        let req = ToolCallRequest {
            request_id: 0,
            tool_name: String::new(),
            operation: String::new(),
            args: vec![(String::new(), String::new())],
        };
        let encoded = encode_tool_call(&req);
        let decoded = decode_tool_call(&encoded).unwrap();
        assert_eq!(req, decoded);
    }
}
