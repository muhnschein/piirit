//! Stand-in model host on loopback: answers each request as scripted, notes its `Range`.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex};

/// How to answer one request.
#[derive(Clone, Copy, Debug)]
pub enum Answer {
    /// 206 from `Range` start if asked, else 200 whole.
    Honour,
    /// 200 whole, `Range` or not.
    IgnoreRange,
    /// 200 announcing whole length, first `n` bytes, then wait for [`ModelServer::release`].
    HoldAfter(usize),
    /// 200 whole, one byte changed (same size, wrong hash).
    Corrupt,
}

struct Shared {
    body: Vec<u8>,
    script: Mutex<VecDeque<Answer>>,
    ranges: Mutex<Vec<Option<String>>>,
    released: Mutex<bool>,
    wake: Condvar,
}

pub struct ModelServer {
    pub url: String,
    shared: Arc<Shared>,
}

impl ModelServer {
    /// Serve `body` at `url`; requests past `script` are honoured.
    pub fn start(body: Vec<u8>, script: Vec<Answer>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let url = format!(
            "http://{}/ggml-test.bin",
            listener.local_addr().expect("bound address")
        );
        let shared = Arc::new(Shared {
            body,
            script: Mutex::new(script.into()),
            ranges: Mutex::new(Vec::new()),
            released: Mutex::new(false),
            wake: Condvar::new(),
        });
        let serving = Arc::clone(&shared);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = Arc::clone(&serving);
                std::thread::spawn(move || answer(&shared, stream));
            }
        });
        Self { url, shared }
    }

    /// Let held answers end (connection closed).
    pub fn release(&self) {
        *self.shared.released.lock().expect("release lock") = true;
        self.shared.wake.notify_all();
    }

    /// `Range` header of each request so far, in order.
    pub fn ranges(&self) -> Vec<Option<String>> {
        self.shared.ranges.lock().expect("ranges lock").clone()
    }
}

fn answer(shared: &Shared, mut stream: TcpStream) {
    let mut range = None;
    {
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        while reader.read_line(&mut line).is_ok_and(|read| read > 0) {
            if line == "\r\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                if name.eq_ignore_ascii_case("range") {
                    range = Some(value.trim().to_owned());
                }
            }
            line.clear();
        }
    }
    shared
        .ranges
        .lock()
        .expect("ranges lock")
        .push(range.clone());
    let step = shared
        .script
        .lock()
        .expect("script lock")
        .pop_front()
        .unwrap_or(Answer::Honour);
    let total = shared.body.len();
    let whole = |body: &[u8]| {
        let mut out =
            format!("HTTP/1.1 200 OK\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n")
                .into_bytes();
        out.extend_from_slice(body);
        out
    };
    let out = match step {
        Answer::Honour => {
            let start = range
                .as_deref()
                .and_then(|range| range.strip_prefix("bytes="))
                .and_then(|range| range.strip_suffix('-'))
                .and_then(|start| start.parse::<usize>().ok());
            match start {
                Some(start) if start < total => {
                    let mut out = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{total}\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n",
                        total - 1,
                        total - start
                    )
                    .into_bytes();
                    out.extend_from_slice(&shared.body[start..]);
                    out
                }
                _ => whole(&shared.body),
            }
        }
        Answer::IgnoreRange => whole(&shared.body),
        Answer::Corrupt => {
            let mut body = shared.body.clone();
            body[total / 2] ^= 0xff;
            whole(&body)
        }
        Answer::HoldAfter(count) => {
            let mut out = whole(&shared.body);
            out.truncate(out.len() - total + count);
            let _ = stream.write_all(&out);
            let _ = stream.flush();
            let mut released = shared.released.lock().expect("release lock");
            while !*released {
                released = shared.wake.wait(released).expect("release wait");
            }
            return;
        }
    };
    let _ = stream.write_all(&out);
    let _ = stream.flush();
}

/// SHA-256, hex, by coreutils' `sha256sum`: independent of app's `QCryptographicHash`.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut child = std::process::Command::new("sha256sum")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("run sha256sum");
    child
        .stdin
        .take()
        .expect("sha256sum stdin")
        .write_all(data)
        .expect("feed sha256sum");
    let output = child.wait_with_output().expect("sha256sum output");
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Bytes that don't repeat: a part from the wrong offset can't pass.
pub fn model_bytes(len: usize) -> Vec<u8> {
    let mut state: u32 = 0x1234_5678;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}
