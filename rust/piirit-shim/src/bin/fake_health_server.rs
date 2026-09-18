//! JSON-RPC double answering `get_system_info` only, for the smoke test.
//! Separate from the transport crate's because `CARGO_BIN_EXE_*` only
//! covers binaries in the same package.

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn main() {
    // By hand rather than `#[tokio::main]`, which is the `macros` feature
    // and a proc-macro crate the app's own build does not need.
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
    let mut stdout = tokio::io::stdout();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        // The real core's event long poll blocks until there is something
        // to say. Answering it with anything else looks like the server
        // died, which is what the client would then report.
        if request.get("method").and_then(Value::as_str) == Some("get_next_event_batch") {
            continue;
        }
        let response = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"name": "fake-health-server", "version": "0.0.0-test"},
        });
        let _ = stdout.write_all(response.to_string().as_bytes()).await;
        let _ = stdout.write_all(b"\n").await;
        let _ = stdout.flush().await;
    }
}
