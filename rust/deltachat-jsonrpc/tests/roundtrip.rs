//! `RpcClient` vs `fake-rpc-test-server`. Any stdio JSON-RPC 2.0 peer proves transport.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use deltachat_jsonrpc::{spawn_event_loop, RpcClient, RpcError};
use serde_json::json;

fn fake_server_path() -> &'static str {
    env!("CARGO_BIN_EXE_fake-rpc-test-server")
}

#[tokio::test]
async fn health_check_round_trip() {
    let client = RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
        .await
        .expect("spawn fake server");

    let info: HashMap<String, String> = client
        .call_unit("get_system_info")
        .await
        .expect("get_system_info call");

    assert_eq!(
        info.get("name").map(String::as_str),
        Some("fake-rpc-test-server")
    );
    assert_eq!(info.get("version").map(String::as_str), Some("0.0.0-test"));

    client.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn echoes_arbitrary_json_params() {
    let client = RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
        .await
        .expect("spawn fake server");

    let payload = json!({"nested": {"a": [1, 2, 3]}, "ok": true});
    let echoed: serde_json::Value = client
        .call("echo", payload.clone())
        .await
        .expect("echo call");

    assert_eq!(echoed, payload);
    client.shutdown().await.expect("shutdown");
}

/// Bad line is not transport close: later answers still arrive.
#[tokio::test]
async fn an_undecodable_line_does_not_end_the_transport() {
    let client = RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
        .await
        .expect("spawn fake server");

    let answer: String = client
        .call_unit("garbage")
        .await
        .expect("the answer written after the undecodable line");
    assert_eq!(answer, "after-garbage");

    let echoed: serde_json::Value = client
        .call("echo", json!("still here"))
        .await
        .expect("a call made after the undecodable line");
    assert_eq!(echoed, json!("still here"));

    client.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn remote_errors_are_reported_as_rpc_error() {
    let client = RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
        .await
        .expect("spawn fake server");

    let result: Result<serde_json::Value, RpcError> = client.call_unit("fail").await;
    match result {
        Err(RpcError::Remote(err)) => {
            assert_eq!(err.code, -32000);
            assert_eq!(err.message, "boom");
        }
        other => panic!("expected RpcError::Remote, got {other:?}"),
    }

    client.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn unknown_method_reports_method_not_found() {
    let client = RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
        .await
        .expect("spawn fake server");

    let result: Result<serde_json::Value, RpcError> = client.call_unit("does_not_exist").await;
    match result {
        Err(RpcError::Remote(err)) => assert_eq!(err.code, -32601),
        other => panic!("expected RpcError::Remote(-32601), got {other:?}"),
    }

    client.shutdown().await.expect("shutdown");
}

/// Calls matched by id, not send order: fast after slow completes first.
#[tokio::test]
async fn concurrent_calls_are_correlated_independently_of_order() {
    let client = Arc::new(
        RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
            .await
            .expect("spawn fake server"),
    );

    // Wide margins: jitter must not fail. Asserts order, not speed.
    let slow_client = client.clone();
    let slow_handle = tokio::spawn(async move {
        let start = Instant::now();
        let result: String = slow_client
            .call("slow", (1500u64,))
            .await
            .expect("slow call");
        assert_eq!(result, "slow-done");
        start.elapsed()
    });

    // Let slow request get in flight first.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let echo_start = Instant::now();
    let _: serde_json::Value = client.call("echo", json!("hi")).await.expect("echo call");
    let echo_elapsed = echo_start.elapsed();

    assert!(
        echo_elapsed < Duration::from_millis(1000),
        "echo took {echo_elapsed:?}, expected it to complete well before the 1.5s slow call"
    );

    let slow_elapsed = slow_handle.await.expect("slow task");
    assert!(slow_elapsed >= Duration::from_millis(1500));

    client.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn event_loop_streams_batches_in_order() {
    let client = Arc::new(
        RpcClient::spawn(fake_server_path(), Vec::<&str>::new())
            .await
            .expect("spawn fake server"),
    );

    let (mut events, handle) = spawn_event_loop(client.clone());

    let first = events.recv().await.expect("first event");
    assert_eq!(first.context_id, 1);
    assert_eq!(first.event["msg"], json!("batch 1"));

    // Six polls answer with errors; stream must survive all. Loop quitting after five ended stream;
    // shim read that as core dead, and app killed it to start another.
    // Backoff: takes seconds.
    let eighth = events
        .recv()
        .await
        .expect("the loop stopped on a run of errors");
    assert_eq!(eighth.event["msg"], json!("batch 8"));

    // Long-poll with no events; stop must not hang.
    handle.stop();
    client.shutdown().await.expect("shutdown");
}
