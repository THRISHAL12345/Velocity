use criterion::{black_box, criterion_group, criterion_main, Criterion};
use velocity_protocol::*;

/// Creates a representative ToolCallRequest matching the benchmark task.
fn sample_tool_call_request() -> ToolCallRequest {
    ToolCallRequest {
        request_id: 42,
        tool_name: "mock_db".to_string(),
        operation: "lookup_account".to_string(),
        args: vec![
            ("account_id".to_string(), "ACC-12345".to_string()),
            ("region".to_string(), "us-east-1".to_string()),
        ],
    }
}

/// Creates a representative ToolCallResponse matching the benchmark task.
fn sample_tool_call_response() -> ToolCallResponse {
    ToolCallResponse {
        request_id: 42,
        success: true,
        payload: r#"{"account":"ACC-12345","name":"Test User","balance":1000.50}"#.to_string(),
        execution_time_us: 7500,
    }
}

fn bench_encode_tool_call(c: &mut Criterion) {
    let req = sample_tool_call_request();
    c.bench_function("encode_tool_call", |b| {
        b.iter(|| encode_tool_call(black_box(&req)))
    });
}

fn bench_decode_tool_call(c: &mut Criterion) {
    let req = sample_tool_call_request();
    let encoded = encode_tool_call(&req);
    c.bench_function("decode_tool_call", |b| {
        b.iter(|| decode_tool_call(black_box(&encoded)))
    });
}

fn bench_round_trip_tool_call(c: &mut Criterion) {
    let req = sample_tool_call_request();
    c.bench_function("round_trip_tool_call", |b| {
        b.iter(|| {
            let encoded = encode_tool_call(black_box(&req));
            decode_tool_call(black_box(&encoded))
        })
    });
}

fn bench_encode_response(c: &mut Criterion) {
    let resp = sample_tool_call_response();
    c.bench_function("encode_response", |b| {
        b.iter(|| encode_response(black_box(&resp)))
    });
}

fn bench_decode_response(c: &mut Criterion) {
    let resp = sample_tool_call_response();
    let encoded = encode_response(&resp);
    c.bench_function("decode_response", |b| {
        b.iter(|| decode_response(black_box(&encoded)))
    });
}

fn bench_round_trip_response(c: &mut Criterion) {
    let resp = sample_tool_call_response();
    c.bench_function("round_trip_response", |b| {
        b.iter(|| {
            let encoded = encode_response(black_box(&resp));
            decode_response(black_box(&encoded))
        })
    });
}

fn bench_encode_heartbeat(c: &mut Criterion) {
    let hb = Heartbeat {
        worker_id: 7,
        timestamp_us: 1_625_000_000_000,
    };
    c.bench_function("encode_heartbeat", |b| {
        b.iter(|| encode_heartbeat(black_box(&hb)))
    });
}

fn bench_round_trip_heartbeat(c: &mut Criterion) {
    let hb = Heartbeat {
        worker_id: 7,
        timestamp_us: 1_625_000_000_000,
    };
    c.bench_function("round_trip_heartbeat", |b| {
        b.iter(|| {
            let encoded = encode_heartbeat(black_box(&hb));
            decode_heartbeat(black_box(&encoded))
        })
    });
}

criterion_group!(
    benches,
    bench_encode_tool_call,
    bench_decode_tool_call,
    bench_round_trip_tool_call,
    bench_encode_response,
    bench_decode_response,
    bench_round_trip_response,
    bench_encode_heartbeat,
    bench_round_trip_heartbeat,
);
criterion_main!(benches);
