//! Test stand-in for `deltachat-rpc-server`: correlation, out-of-order replies, errors, long poll.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, Notify};

fn main() {
    // Not `#[tokio::main]`: needs `macros`, a proc-macro crate.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("fake server: cannot build a runtime: {err}");
            std::process::exit(1);
        }
    };
    runtime.block_on(serve());
}

async fn serve() {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
    let batch_counter = Arc::new(Mutex::new(0u32));
    let released = Arc::new(Notify::new());

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let stdout = stdout.clone();
        let batch_counter = batch_counter.clone();
        let released = released.clone();
        tokio::spawn(async move {
            let request: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => return,
            };
            let id = request.get("id").cloned();
            let method = request
                .get("method")
                .and_then(|m| m.as_str())
                .unwrap_or_default()
                .to_string();
            let params = request.get("params").cloned().unwrap_or(Value::Null);

            let response = match method.as_str() {
                "get_system_info" => ok(
                    id,
                    &json!({"name": "fake-rpc-test-server", "version": "0.0.0-test"}),
                ),
                "echo" => ok(id, &params),
                "add" => {
                    let nums = params.as_array().cloned().unwrap_or_default();
                    let sum: f64 = nums.iter().filter_map(Value::as_f64).sum();
                    ok(id, &json!(sum))
                }
                "fail" => err(id, -32000, "boom"),
                // Non-UTF-8 line: one bad line, not closed pipe.
                "garbage" => {
                    let mut out = stdout.lock().await;
                    let _ = out.write_all(b"\xff\xfe this is not text\n").await;
                    let _ = out.flush().await;
                    drop(out);
                    ok(id, &json!("after-garbage"))
                }
                // Answered once `release` came: order set by test, not clock.
                "hold" => {
                    released.notified().await;
                    ok(id, &json!("held-done"))
                }
                "release" => {
                    released.notify_one();
                    ok(id, &json!("released"))
                }
                "get_next_event_batch" => {
                    let mut count = batch_counter.lock().await;
                    *count += 1;
                    // Six unusable answers (core changing event shape): stream must survive.
                    if (2..=7).contains(&*count) {
                        err(id, -32000, "no events for you")
                    } else if *count <= 8 {
                        let n = *count;
                        ok(
                            id,
                            &json!([{
                                "contextId": 1,
                                "event": {"kind": "Info", "msg": format!("batch {n}")},
                            }]),
                        )
                    } else {
                        // Blocks forever, like real long poll.
                        drop(count);
                        std::future::pending::<()>().await;
                        unreachable!("pending future never resolves")
                    }
                }
                _ => err(id, -32601, "method not found"),
            };

            if let Some(response) = response {
                let mut out = stdout.lock().await;
                let _ = out.write_all(response.to_string().as_bytes()).await;
                let _ = out.write_all(b"\n").await;
                let _ = out.flush().await;
            }
        });
    }
}

fn ok(id: Option<Value>, result: &Value) -> Option<Value> {
    id.map(|id| json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn err(id: Option<Value>, code: i64, message: &str) -> Option<Value> {
    id.map(|id| json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}))
}
