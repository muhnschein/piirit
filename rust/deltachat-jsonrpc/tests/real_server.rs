//! Integration test vs REAL `deltachat-rpc-server` binary.
//!
//! Milestone 1 check plus offline account/chat ops. Proves transport vs real core.
//!
//! Gated on `DELTACHAT_RPC_SERVER`; skipped otherwise.
//!
//! Offline: sender's mail is a stub on 127.0.0.1, IO stopped after. Sends to Saved Messages only.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use deltachat_jsonrpc::{spawn_event_loop, RpcClient, RpcError};
use serde_json::Value;

/// 1x1 PNG with real pixels, not a name to guess from.
const ONE_PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

/// 1x1 GIF: core reports no dimensions. Conversation must cope.
const ONE_PIXEL_GIF: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xff, 0xff, 0xff,
    0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b,
];

/// 8 kHz 8-bit mono silence as WAV. Built, so sizes are arithmetic.
fn one_second_wav() -> Vec<u8> {
    let (rate, bits, channels) = (8000_u32, 8_u16, 1_u16);
    // One byte per sample: a second is `rate` bytes.
    let bytes = rate;
    let samples = vec![0x80_u8; bytes as usize];
    let mut wav = Vec::with_capacity(44 + samples.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes()); // bytes per second
    wav.extend_from_slice(&1_u16.to_le_bytes()); // block align
    wav.extend_from_slice(&bits.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&bytes.to_le_bytes());
    wav.extend_from_slice(&samples);
    wav
}

/// One second of silence as recorder MP3: MPEG-2 layer III, 16 kHz, mono, 32 kbit/s CBR. Each
/// frame: same 4-byte header + 140 zero bytes, 576 samples; as many frames as a second takes.
fn one_second_mp3() -> Vec<u8> {
    let mut frame = [0_u8; 144];
    frame[..4].copy_from_slice(&[0xFF, 0xF3, 0x48, 0xC0]);
    frame.repeat(16_000_usize.div_ceil(576))
}

/// Plain PNG, `side` px square, stored uncompressed.
///
/// Core recodes past `BALANCED_IMAGE_BYTES` to JPEG, scaling only past quality's side limit (core
/// 2.61+: 1760 px balanced, 640 worse), so 1400 px keeps size at balanced, shrinks at worse.
/// Stored uncompressed: no deflate impl worth carrying for a fixture.
fn plain_png(side: usize) -> Vec<u8> {
    /// PNG length field.
    fn span(value: usize) -> u32 {
        u32::try_from(value).unwrap_or(u32::MAX)
    }

    // Gradient: recoded output must shrink; JPEG is worst at noise.
    let mut raw = Vec::with_capacity(side * (side * 3 + 1));
    for row in 0..side {
        raw.push(0); // filter: none
        for column in 0..side {
            let shade = u8::try_from((row + column) * 255 / (2 * side)).unwrap_or(255);
            raw.extend_from_slice(&[shade, shade / 2, 255 - shade]);
        }
    }

    // zlib: header, stored deflate blocks, Adler-32.
    let mut zlib = vec![0x78, 0x01];
    let mut adler: (u32, u32) = (1, 0);
    for byte in &raw {
        adler.0 = (adler.0 + u32::from(*byte)) % 65521;
        adler.1 = (adler.1 + adler.0) % 65521;
    }
    let mut rest = raw.as_slice();
    while !rest.is_empty() {
        let take = rest.len().min(65535);
        let (block, left) = rest.split_at(take);
        let last = u8::from(left.is_empty());
        let len = u16::try_from(take).unwrap_or(u16::MAX);
        zlib.push(last);
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
        rest = left;
    }
    zlib.extend_from_slice(&((adler.1 << 16) | adler.0).to_be_bytes());

    // PNG chunk: length, name, bytes, CRC-32 (as zip).
    let chunk = |kind: &[u8], data: &[u8]| {
        let mut out = Vec::with_capacity(12 + data.len());
        out.extend_from_slice(&span(data.len()).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let mut crc_over = kind.to_vec();
        crc_over.extend_from_slice(data);
        out.extend_from_slice(&crc32(&crc_over).to_be_bytes());
        out
    };
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&span(side).to_be_bytes());
    header.extend_from_slice(&span(side).to_be_bytes());
    // 8-bit, truecolour, no interlace.
    header.extend_from_slice(&[8, 2, 0, 0, 0]);

    let mut png = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    png.extend_from_slice(&chunk(b"IHDR", &header));
    png.extend_from_slice(&chunk(b"IDAT", &zlib));
    png.extend_from_slice(&chunk(b"IEND", &[]));
    png
}

/// Send file to `chat`; return stored message.
async fn send_file(
    client: &RpcClient,
    account_id: u32,
    chat_id: u32,
    path: &std::path::Path,
    name: &str,
) -> Value {
    let (message_id, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                account_id,
                chat_id,
                Option::<String>::None,
                Some(path.to_string_lossy().into_owned()),
                Some(name.to_string()),
                Option::<(f64, f64)>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .unwrap_or_else(|err| panic!("misc_send_msg for {name}: {err}"));
    let messages: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (account_id, vec![message_id]))
        .await
        .unwrap_or_else(|err| panic!("get_messages for {name}: {err}"));
    messages[&message_id].clone()
}

/// CRC-32 as zip stores it, so readers accept the archive.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            // Polynomial where low bit set.
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// Zip length field: overflow here is a bug, not an error.
fn field32(value: usize) -> u32 {
    match u32::try_from(value) {
        Ok(value) => value,
        Err(err) => panic!("the archive does not fit a zip: {err}"),
    }
}

/// Same, for half-width fields.
fn field16(value: usize) -> u16 {
    match u16::try_from(value) {
        Ok(value) => value,
        Err(err) => panic!("the archive does not fit a zip: {err}"),
    }
}

/// `.xdc`: zip of stored entries. Built, not checked in. Only core reads it.
fn webxdc(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (name, body) in entries {
        let offset = field32(out.len());
        let crc = crc32(body);
        let size = field32(body.len());
        let name_len = field16(name.len());
        // Local file header, then bytes.
        out.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        for field in [20_u16, 0, 0, 0, 0x21] {
            // version, flags, method (stored), time, date
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(body);

        // Central directory entry: what readers look at first.
        directory.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        for field in [20_u16, 20, 0, 0, 0, 0x21] {
            // made by, version, flags, method, time, date
            directory.extend_from_slice(&field.to_le_bytes());
        }
        directory.extend_from_slice(&crc.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&name_len.to_le_bytes());
        for field in [0_u16, 0, 0, 0] {
            // extra, comment, disk, attributes
            directory.extend_from_slice(&field.to_le_bytes());
        }
        directory.extend_from_slice(&0_u32.to_le_bytes());
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());
    }
    let directory_at = field32(out.len());
    let directory_size = field32(directory.len());
    let count = field16(entries.len());
    out.extend_from_slice(&directory);
    out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    for field in [0_u16, 0, count, count] {
        // disk numbers, entry counts
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&directory_size.to_le_bytes());
    out.extend_from_slice(&directory_at.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

/// Base64 without padding, as core hands blobs over (`webxdc_host.rs`).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let mut block = [0_u8; 3];
        block[..chunk.len()].copy_from_slice(chunk);
        let packed = (u32::from(block[0]) << 16) | (u32::from(block[1]) << 8) | u32::from(block[2]);
        for index in 0..=chunk.len() {
            out.push(char::from(
                ALPHABET[((packed >> (18 - 6 * index)) & 0x3f) as usize],
            ));
        }
    }
    out
}

/// Mail server that says yes, on two loopback ports: IMAP, then SMTP.
///
/// Core 2.61+ refuses `configured_addr` without a transport; JSON-RPC cannot add one without
/// login. IMAP answers each tagged line `OK`, SMTP each step its reply. IO stopped once in.
///
/// Threads not joined; listeners die with process.
fn loopback_mailbox() -> (u16, u16) {
    let imap = listen("* OK ready\r\n", |line| {
        let tag = line.split(' ').next().unwrap_or_default();
        format!("{tag} OK\r\n")
    });
    let smtp = listen("220 ready\r\n", |line| {
        match line.get(..4).map(str::to_ascii_uppercase).as_deref() {
            Some("EHLO") => "250 AUTH PLAIN\r\n",
            Some("AUTH") => "235 ok\r\n",
            Some("QUIT") => "221 bye\r\n",
            _ => "250 ok\r\n",
        }
        .to_string()
    });
    (imap, smtp)
}

/// Loopback port: `greeting`, then `answer` per line. Returns port.
fn listen(greeting: &'static str, answer: fn(&str) -> String) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .unwrap_or_else(|err| panic!("no loopback port for the stub mail server: {err}"));
    let port = listener
        .local_addr()
        .unwrap_or_else(|err| panic!("the stub mail server has no address: {err}"))
        .port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            std::thread::spawn(move || converse(&stream, greeting, answer));
        }
    });
    port
}

/// One connection of `listen`. Any error ends it.
fn converse(stream: &TcpStream, greeting: &str, answer: fn(&str) -> String) -> std::io::Result<()> {
    let quiet = Some(Duration::from_secs(10));
    stream.set_read_timeout(quiet)?;
    stream.set_write_timeout(quiet)?;
    let mut out = stream;
    out.write_all(greeting.as_bytes())?;
    for line in BufReader::new(stream).lines() {
        out.write_all(answer(&line?).as_bytes())?;
    }
    Ok(())
}

/// DCLOGIN code for `addr`, pointed at `loopback_mailbox` ports.
fn dclogin(addr: &str, (imap_port, smtp_port): (u16, u16)) -> String {
    format!(
        "dclogin:{addr}?p=x&v=1&ih=127.0.0.1&ip={imap_port}\
         &is=plain&sh=127.0.0.1&sp={smtp_port}&ss=plain"
    )
}

/// Core's words for a refused call, as pages show them.
async fn refusal<P: serde::Serialize>(client: &RpcClient, method: &str, params: P) -> String {
    match client.call::<_, Value>(method, params).await {
        Err(RpcError::Remote(refused)) => refused.message,
        other => panic!("the core did not refuse {method}: {other:?}"),
    }
}

/// Resolve gate value; relative paths against repo root.
///
/// Cargo runs tests from *package* root, so `DELTACHAT_RPC_SERVER=../vendor/...` would look
/// under `rust/`. Fails only in CI, reads as missing download.
fn resolve(path: &str) -> String {
    let path = std::path::Path::new(path);
    if path.is_absolute() {
        return path.to_string_lossy().into_owned();
    }
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
        .to_string_lossy()
        .into_owned()
}

/// Core version pinned in `scripts/fetch-rpc-server.sh`.
fn pinned_core_version() -> String {
    let script = std::fs::read_to_string(resolve("scripts/fetch-rpc-server.sh"))
        .unwrap_or_else(|err| panic!("read scripts/fetch-rpc-server.sh: {err}"));
    script
        .lines()
        .find_map(|line| line.strip_prefix("VERSION=\""))
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| panic!("scripts/fetch-rpc-server.sh has no VERSION=\"...\" line"))
        .to_string()
}

fn real_server() -> Option<String> {
    match std::env::var("DELTACHAT_RPC_SERVER") {
        Ok(path) if !path.is_empty() => Some(resolve(&path)),
        _ => {
            eprintln!(
                "skipping: set DELTACHAT_RPC_SERVER to a real deltachat-rpc-server binary to run"
            );
            None
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
// One long fn on purpose: one live session, in order. Splitting respawns core per step.
#[allow(clippy::too_many_lines)]
async fn offline_round_trip_against_real_core() {
    let Some(server) = real_server() else {
        return;
    };

    let accounts_dir =
        std::env::temp_dir().join(format!("piirit-real-server-test-{}", std::process::id()));
    std::fs::create_dir_all(&accounts_dir).expect("create accounts dir");

    let client = Arc::new(
        RpcClient::spawn_with_env(
            &server,
            Vec::<&str>::new(),
            [("DC_ACCOUNTS_PATH", accounts_dir.as_os_str())],
        )
        .await
        .expect("spawn real deltachat-rpc-server"),
    );

    // Milestone 1 check: get_system_info answers over stdio.
    let info: std::collections::BTreeMap<String, String> = client
        .call_unit("get_system_info")
        .await
        .expect("get_system_info");
    assert!(
        info.contains_key("deltachat_core_version"),
        "unexpected get_system_info keys: {:?}",
        info.keys().collect::<Vec<_>>()
    );
    // Pins the RPM's core: binary version = fetch script's pin = SOURCE.md tag. A bump missing one
    // stops here.
    let pinned = pinned_core_version();
    assert_eq!(
        info.get("deltachat_core_version").map(String::as_str),
        Some(format!("v{pinned}").as_str()),
        "the server under test is not the core scripts/fetch-rpc-server.sh pins"
    );
    let notice = std::fs::read_to_string(resolve("vendor/deltachat-rpc-server/SOURCE.md"))
        .expect("read vendor/deltachat-rpc-server/SOURCE.md");
    assert!(
        notice.contains(&format!("**Version / tag:** `v{pinned}`"))
            && notice.contains(&format!("deltachat-rpc-server=={pinned}")),
        "vendor/deltachat-rpc-server/SOURCE.md does not describe v{pinned}"
    );

    // Account bootstrap, as DeltaChatCore::add_account.
    let account_id: u32 = client.call_unit("add_account").await.expect("add_account");

    // Config round trip, as configure_account (no `configure`: needs live mail).
    client
        .call::<_, ()>(
            "set_config",
            (account_id, "displayname", Some("Piirit Test")),
        )
        .await
        .expect("set_config");
    let name: Option<String> = client
        .call("get_config", (account_id, "displayname"))
        .await
        .expect("get_config");
    assert_eq!(name.as_deref(), Some("Piirit Test"));

    // Start event stream before anything emits one.
    let (mut events, handle) = spawn_event_loop(client.clone());

    // Draft emits MsgsChanged: proves event delivery end to end.
    let contact_id: u32 = client
        .call(
            "create_contact",
            (account_id, "test@example.org", Some("Testy")),
        )
        .await
        .expect("create_contact");
    let chat_id: u32 = client
        .call("create_chat_by_contact_id", (account_id, contact_id))
        .await
        .expect("create_chat_by_contact_id");

    client
        .call::<_, ()>(
            "misc_set_draft",
            (
                account_id,
                chat_id,
                Some("draft from integration test"),
                Option::<String>::None,
                Option::<String>::None,
                Option::<u32>::None,
                Option::<String>::None,
            ),
        )
        .await
        .expect("misc_set_draft");

    let mut saw_msgs_changed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(event)) => {
                if event.context_id == account_id
                    && event.event.get("kind").and_then(Value::as_str) == Some("MsgsChanged")
                {
                    saw_msgs_changed = true;
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_msgs_changed,
        "no MsgsChanged event arrived after setting a draft"
    );

    // Chat list holds the chat just created, via fetch_chat_list's calls.
    let entries: Vec<u32> = client
        .call(
            "get_chatlist_entries",
            (
                account_id,
                Option::<u32>::None,
                Option::<String>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .expect("get_chatlist_entries");
    assert!(
        entries.contains(&chat_id),
        "chat {chat_id} missing from chat list {entries:?}"
    );
    let items: std::collections::HashMap<u32, Value> = client
        .call("get_chatlist_items_by_entries", (account_id, vec![chat_id]))
        .await
        .expect("get_chatlist_items_by_entries");
    let item = &items[&chat_id];
    assert_eq!(
        item.get("kind").and_then(Value::as_str),
        Some("ChatListItem")
    );
    assert_eq!(item.get("name").and_then(Value::as_str), Some("Testy"));

    // Draft summary: `summaryText1` prefix shows before `summaryText2`. `summaryStatus` 19 =
    // DC_STATE_OUT_DRAFT; under 20 means no delivery mark.
    assert_eq!(
        item.get("summaryText1").and_then(Value::as_str),
        Some("Draft"),
        "a chat with a draft does not name it in its summary, so the chat \
         list cannot say which chats are holding one: {item:?}"
    );
    assert_eq!(
        item.get("summaryText2").and_then(Value::as_str),
        Some("draft from integration test"),
        "the draft's own text is not what the chat list previews: {item:?}"
    );
    assert_eq!(
        item.get("summaryStatus").and_then(Value::as_u64),
        Some(19),
        "a draft's summary state is not DC_STATE_OUT_DRAFT, so the row \
         cannot tell it from something sent: {item:?}"
    );

    // Reader's language replaces "Draft" via `set_stock_strings` (id 3). Restored after.
    let stock = |text: &str| std::collections::BTreeMap::from([(3_u32, text.to_string())]);
    client
        .call::<_, ()>("set_stock_strings", (stock("Entwurf"),))
        .await
        .expect("set_stock_strings");
    let translated: std::collections::HashMap<u32, Value> = client
        .call("get_chatlist_items_by_entries", (account_id, vec![chat_id]))
        .await
        .expect("get_chatlist_items_by_entries after set_stock_strings");
    assert_eq!(
        translated[&chat_id]
            .get("summaryText1")
            .and_then(Value::as_str),
        Some("Entwurf"),
        "the core did not take a stock string in the shape the app sends, so \
         every word it writes stays English: {:?}",
        translated[&chat_id]
    );
    client
        .call::<_, ()>("set_stock_strings", (stock("Draft"),))
        .await
        .expect("set_stock_strings back");

    // Read draft back: survives app close. Comes back as message object.
    let draft: Value = client
        .call("get_draft", (account_id, chat_id))
        .await
        .expect("get_draft");
    assert_eq!(
        draft.get("text").and_then(Value::as_str),
        Some("draft from integration test"),
        "the draft did not come back: {draft:?}"
    );

    // Clear: a call for it, answers null, not empty message.
    client
        .call::<_, Value>("remove_draft", (account_id, chat_id))
        .await
        .expect("remove_draft");
    let gone: Value = client
        .call("get_draft", (account_id, chat_id))
        .await
        .expect("get_draft after remove");
    assert!(gone.is_null(), "a removed draft still reads back: {gone:?}");

    // Message-list shape: `kind` "message", id `msg_id` (snake_case, was a bug).
    let msg_items: Vec<Value> = client
        .call(
            "get_message_list_items",
            (account_id, chat_id, false, false),
        )
        .await
        .expect("get_message_list_items");
    for item in &msg_items {
        let kind = item.get("kind").and_then(Value::as_str);
        assert!(
            kind == Some("message") || kind == Some("dayMarker"),
            "unexpected message list item kind: {item:?}"
        );
        if kind == Some("message") {
            assert!(
                item.get("msg_id").is_some(),
                "expected snake_case msg_id in {item:?}"
            );
        }
    }

    // Account list: `kind` tag, "Configured"/"Unconfigured", camelCase fields.
    let accounts: Vec<Value> = client
        .call_unit("get_all_accounts")
        .await
        .expect("get_all_accounts");
    let ours = accounts
        .iter()
        .find(|a| a.get("id").and_then(Value::as_u64) == Some(u64::from(account_id)))
        .expect("our account missing from get_all_accounts");
    assert_eq!(
        ours.get("kind").and_then(Value::as_str),
        Some("Unconfigured"),
        "unexpected account shape: {ours:?}"
    );

    // Selected profile: core auto-selects a new account; `select_account` takes bare id;
    // `get_selected_account_id` gives id or null (`Option<u32>`, as shim decodes).
    let selected: Option<u32> = client
        .call_unit("get_selected_account_id")
        .await
        .expect("get_selected_account_id");
    assert_eq!(
        selected,
        Some(account_id),
        "the core did not select the account it had just added"
    );
    client
        .call::<_, ()>("select_account", (account_id,))
        .await
        .expect("select_account");

    // DCACCOUNT: parsed locally: kind "account", snake_case fields.
    let qr: Value = client
        .call(
            "check_qr",
            (account_id, "DCACCOUNT:https://nine.testrun.org/new"),
        )
        .await
        .expect("check_qr");
    assert_eq!(
        qr.get("kind").and_then(Value::as_str),
        Some("account"),
        "unexpected qr shape: {qr:?}"
    );
    assert_eq!(
        qr.get("domain").and_then(Value::as_str),
        Some("nine.testrun.org"),
        "unexpected qr shape: {qr:?}"
    );

    // Encryption flags as models read them: address chat unencrypted, group encrypted.
    assert_eq!(
        items[&chat_id].get("isEncrypted").and_then(Value::as_bool),
        Some(false),
        "plain-email chat should be unencrypted: {:?}",
        items[&chat_id]
    );
    let group_chat_id: u32 = client
        .call("create_group_chat", (account_id, "Test Group", false))
        .await
        .expect("create_group_chat");
    let group_items: std::collections::HashMap<u32, Value> = client
        .call(
            "get_chatlist_items_by_entries",
            (account_id, vec![group_chat_id]),
        )
        .await
        .expect("get_chatlist_items_by_entries for group");
    assert_eq!(
        group_items[&group_chat_id]
            .get("isEncrypted")
            .and_then(Value::as_bool),
        Some(true),
        "fresh group chat should be encrypted: {:?}",
        group_items[&group_chat_id]
    );

    // Group as ChatInfo reads it. Members are ids; contacts keyed by id.
    let full: Value = client
        .call("get_full_chat_by_id", (account_id, group_chat_id))
        .await
        .expect("get_full_chat_by_id");
    assert_eq!(
        full.get("chatType").and_then(Value::as_str),
        Some("Group"),
        "unexpected chat shape: {full:?}"
    );
    // Description: stored as typed, less outer whitespace; empty clears.
    let description = |text: &'static str| {
        let client = client.clone();
        async move {
            client
                .call::<_, ()>("set_chat_description", (account_id, group_chat_id, text))
                .await
                .expect("set_chat_description");
            client
                .call::<_, String>("get_chat_description", (account_id, group_chat_id))
                .await
                .expect("get_chat_description")
        }
    };
    assert_eq!(
        description("Walks\nBring boots").await,
        "Walks\nBring boots"
    );
    assert_eq!(
        description("").await,
        "",
        "an empty description does not clear it"
    );
    // `canSend` and kind decide if a call can be placed (`takes_calls`).
    for flag in ["isEncrypted", "isSelfTalk", "isDeviceChat", "canSend"] {
        assert!(
            full.get(flag).is_some_and(Value::is_boolean),
            "the full chat carries no {flag}: {full:?}"
        );
    }
    let member_ids: Vec<u32> =
        serde_json::from_value(full["contactIds"].clone()).expect("contactIds is a list of ids");
    assert_eq!(
        member_ids,
        vec![1],
        "a fresh group holds only the account itself (DC_CONTACT_ID_SELF): {full:?}"
    );
    for field in ["name", "profileImage", "color", "selfInGroup", "canSend"] {
        assert!(
            full.get(field).is_some(),
            "the chat lost the {field} field: {full:?}"
        );
    }
    assert_eq!(
        full.get("selfInGroup").and_then(Value::as_bool),
        Some(true),
        "the account is not in the group it just made: {full:?}"
    );
    let members: std::collections::HashMap<u32, Value> = client
        .call("get_contacts_by_ids", (account_id, member_ids))
        .await
        .expect("get_contacts_by_ids");
    let me = &members[&1];
    assert!(
        me.get("displayName").is_some() && me.get("address").is_some(),
        "the account's own contact is not shaped like a contact: {me:?}"
    );
    // Each edit its own call. Address contact can't join encrypted group: refused member.
    client
        .call::<_, ()>(
            "set_chat_name",
            (account_id, group_chat_id, "Renamed Group"),
        )
        .await
        .expect("set_chat_name");
    let renamed: Value = client
        .call("get_full_chat_by_id", (account_id, group_chat_id))
        .await
        .expect("get_full_chat_by_id after rename");
    assert_eq!(
        renamed.get("name").and_then(Value::as_str),
        Some("Renamed Group"),
        "the rename did not take: {renamed:?}"
    );
    assert!(
        client
            .call::<_, ()>("set_chat_name", (account_id, group_chat_id, ""))
            .await
            .is_err(),
        "the core accepted an empty group name"
    );
    assert!(
        client
            .call::<_, ()>(
                "add_contact_to_chat",
                (account_id, group_chat_id, contact_id)
            )
            .await
            .is_err(),
        "an address contact went into an encrypted group; the shim's \
         refused-member reporting is built on this being refused"
    );
    // Disappearing messages: seconds, read back as `ephemeralTimer`.
    client
        .call::<_, ()>(
            "set_chat_ephemeral_timer",
            (account_id, group_chat_id, 3600),
        )
        .await
        .expect("set_chat_ephemeral_timer");
    let timed: Value = client
        .call("get_full_chat_by_id", (account_id, group_chat_id))
        .await
        .expect("get_full_chat_by_id after the timer");
    assert_eq!(
        timed.get("ephemeralTimer").and_then(Value::as_u64),
        Some(3600),
        "the timer did not come back on the chat: {timed:?}"
    );
    // Removing non-member: no error. null clears picture, even if none.
    client
        .call::<_, ()>(
            "remove_contact_from_chat",
            (account_id, group_chat_id, contact_id),
        )
        .await
        .expect("remove_contact_from_chat of a non-member");
    client
        .call::<_, ()>(
            "set_chat_profile_image",
            (account_id, group_chat_id, Option::<String>::None),
        )
        .await
        .expect("set_chat_profile_image with null");

    // Clear unread badge, as open_chat does. Pins method vs real core.
    client
        .call::<_, ()>("marknoticed_chat", (account_id, chat_id))
        .await
        .expect("marknoticed_chat");

    // Read position for "new messages" line. Nothing unseen: null, not zero.
    let nothing: Option<u32> = client
        .call("get_first_unread_message_of_chat", (account_id, chat_id))
        .await
        .expect("get_first_unread_message_of_chat on a read chat");
    assert_eq!(
        nothing, None,
        "a chat with nothing unread named a message to draw the line above"
    );

    // Device message is the one incoming message core takes offline: fresh.
    let device_message: Option<u32> = client
        .call(
            "add_device_message",
            (
                account_id,
                "piirit-unread-probe",
                serde_json::json!({"text": "something new"}),
            ),
        )
        .await
        .expect("add_device_message");
    let device_message = device_message.expect("the device message was not added");
    let device_chat = client
        .call::<_, Value>("get_message", (account_id, device_message))
        .await
        .expect("get_message for the device message")
        .get("chatId")
        .and_then(Value::as_u64)
        .and_then(|id| u32::try_from(id).ok())
        .expect("the device message has no chat");
    let unread: Option<u32> = client
        .call(
            "get_first_unread_message_of_chat",
            (account_id, device_chat),
        )
        .await
        .expect("get_first_unread_message_of_chat on an unread chat");
    assert_eq!(
        unread,
        Some(device_message),
        "the core did not name the first unseen message of the chat"
    );

    // Message as conversation reads it. Saved Messages: only chat sending offline.
    //
    // Sender configured vs loopback stub (as signup.rs). `bcc_self` off: not queued.
    let sender_id: u32 = client
        .call_unit("add_account")
        .await
        .expect("add_account for the message probe");
    client
        .call::<_, ()>("set_config", (sender_id, "displayname", "Testy"))
        .await
        .expect("set_config displayname");
    let mailbox = loopback_mailbox();
    client
        .call::<_, ()>(
            "add_transport_from_qr",
            (sender_id, dclogin("self@example.invalid", mailbox)),
        )
        .await
        .expect("add_transport_from_qr against the loopback stub");
    // Transport starts account IO; would keep talking to stub for rest of test.
    client
        .call::<_, ()>("stop_io", (sender_id,))
        .await
        .expect("stop_io");
    // Configure leaves fresh welcome messages in device chat (contact 5); noticed here so later
    // unread counts start at zero.
    let welcome: Option<u32> = client
        .call("get_chat_id_by_contact_id", (sender_id, 5))
        .await
        .expect("get_chat_id_by_contact_id for the device chat");
    if let Some(device) = welcome {
        client
            .call::<_, ()>("marknoticed_chat", (sender_id, device))
            .await
            .expect("marknoticed_chat on the device chat");
    }
    let saved: u32 = client
        .call("create_chat_by_contact_id", (sender_id, 1))
        .await
        .expect("create_chat_by_contact_id for self");
    let (first, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                sender_id,
                saved,
                Some("hello there"),
                Option::<String>::None,
                Option::<String>::None,
                Option::<(f64, f64)>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .expect("misc_send_msg");
    let attachment = std::env::temp_dir().join("piirit-real-server-note.txt");
    std::fs::write(&attachment, b"hi").expect("write attachment");
    let (second, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                sender_id,
                saved,
                Some("a reply"),
                Some(attachment.to_string_lossy().into_owned()),
                Some("note.txt".to_string()),
                Option::<(f64, f64)>::None,
                // Quote first message.
                Some(first),
            ),
        )
        .await
        .expect("misc_send_msg with a quote and a file");
    let messages: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (sender_id, vec![first, second]))
        .await
        .expect("get_messages");
    // Core 2.64+: a message's summary text, as notifications and pinned
    // messages word it. Unused so far; here so a later move to it starts
    // from what the core gives.
    let summary: String = client
        .call(
            "get_message_summary_text",
            (sender_id, first, false, false, 0u32),
        )
        .await
        .expect("get_message_summary_text");
    assert_eq!(summary, "hello there");
    let reply = &messages[&second];
    // Every field the message rows are built from.
    assert_eq!(
        reply.get("viewType").and_then(Value::as_str),
        Some("File"),
        "unexpected message shape: {reply:?}"
    );
    assert_eq!(
        reply.get("fileName").and_then(Value::as_str),
        Some("note.txt"),
        "unexpected message shape: {reply:?}"
    );
    for field in [
        "file",
        "isInfo",
        "timestamp",
        "state",
        "showPadlock",
        "text",
    ] {
        assert!(
            reply.get(field).is_some(),
            "message lost the {field} field: {reply:?}"
        );
    }
    assert!(
        reply.pointer("/sender/displayName").is_some() && reply.pointer("/sender/color").is_some(),
        "message says nothing about its sender: {reply:?}"
    );
    assert_eq!(
        reply.pointer("/quote/text").and_then(Value::as_str),
        Some("hello there"),
        "the quoted message did not come back with the quote: {reply:?}"
    );
    assert!(
        reply.pointer("/quote/authorDisplayName").is_some(),
        "the quote says nothing about its author: {reply:?}"
    );

    // Long message: core cuts body, rest in HTML part (`hasHtml`). Cut at 38 lines.
    let mut long_body = String::new();
    for number in 1..=50 {
        use std::fmt::Write as _;
        let _ = writeln!(
            long_body,
            "line {number} of a message nobody would call short"
        );
    }
    let (fourth, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                sender_id,
                saved,
                Some(long_body.clone()),
                Option::<String>::None,
                Option::<String>::None,
                Option::<(f64, f64)>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .expect("misc_send_msg with a long body");
    let long_message: Value = client
        .call("get_message", (sender_id, fourth))
        .await
        .expect("get_message");
    assert_eq!(
        long_message.get("hasHtml").and_then(Value::as_bool),
        Some(true),
        "the core did not mark a fifty-line message as one it cut, so          nothing in the app can tell that the reader is being shown only          part of it: {long_message:?}"
    );
    let shown = long_message
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        shown.len() < long_body.len(),
        "the core kept the whole body in `text` after marking the          message as cut, which would make every offer to show the rest          of it a no-op: {shown:?}"
    );
    let whole: Option<String> = client
        .call("get_message_html", (sender_id, fourth))
        .await
        .expect("get_message_html");
    let whole = whole.unwrap_or_default();
    assert!(
        whole.contains("line 50"),
        "the message's HTML part does not hold the end of the body, so          there is nowhere left to read it from: {whole:?}"
    );
    // Short message whole. Asking for missing HTML part is no error.
    let short_message: Value = client
        .call("get_message", (sender_id, first))
        .await
        .expect("get_message for a short one");
    assert_eq!(
        short_message.get("hasHtml").and_then(Value::as_bool),
        Some(false),
        "a three-word message was marked as one the core had to cut:          {short_message:?}"
    );

    // Core types view from file; app classifies no attachment itself.
    let picture = std::env::temp_dir().join("piirit-real-server-dot.png");
    // Smallest valid PNG: real pixels to read.
    std::fs::write(&picture, ONE_PIXEL_PNG).expect("write picture");
    let (third, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                sender_id,
                saved,
                Option::<String>::None,
                Some(picture.to_string_lossy().into_owned()),
                Some("dot.png".to_string()),
                Option::<(f64, f64)>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .expect("misc_send_msg with a picture");
    let messages: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (sender_id, vec![third]))
        .await
        .expect("get_messages");
    let sent_picture = &messages[&third];
    assert_eq!(
        sent_picture.get("viewType").and_then(Value::as_str),
        Some("Image"),
        "the core did not classify a PNG as an image, so the conversation \
         would show it as a paperclip: {sent_picture:?}"
    );
    for field in ["dimensionsWidth", "dimensionsHeight"] {
        assert_eq!(
            sent_picture.get(field).and_then(Value::as_i64),
            Some(1),
            "the core reported no {field}, which is what the delegate sizes \
             the picture from: {sent_picture:?}"
        );
    }

    // Core keeps `File` at size given, recodes `Image`. Pictures named before send
    // (piirit-shim/src/media.rs), else quality setting does nothing.
    let wide = std::env::temp_dir().join("piirit-real-server-wide.png");
    let wide_bytes = plain_png(1400);
    std::fs::write(&wide, &wide_bytes).expect("write the big picture");
    let untouched = send_file(&client, sender_id, saved, &wide, "wide.png").await;
    assert_eq!(
        untouched.get("fileBytes").and_then(Value::as_u64),
        u64::try_from(wide_bytes.len()).ok(),
        "the core recoded a picture sent as a file, which is the one way \
         left to send one at its original size: {untouched:?}"
    );

    let as_image = |quality: &'static str| {
        let client = &client;
        let wide = wide.clone();
        async move {
            client
                .call::<_, ()>("set_config", (sender_id, "media_quality", Some(quality)))
                .await
                .expect("set_config media_quality");
            let message_id: u32 = client
                .call(
                    "send_msg",
                    (
                        sender_id,
                        saved,
                        serde_json::json!({
                            "text": Option::<String>::None,
                            "file": wide.to_string_lossy(),
                            "filename": "wide.png",
                            "viewtype": "Image",
                            "quotedMessageId": Option::<u32>::None,
                        }),
                    ),
                )
                .await
                .expect("send_msg with an Image view type");
            let messages: std::collections::HashMap<u32, Value> = client
                .call("get_messages", (sender_id, vec![message_id]))
                .await
                .expect("get_messages for the recoded picture");
            messages[&message_id].clone()
        }
    };

    // 0 = "balanced" in settings; 1 = "worse quality".
    let balanced = as_image("0").await;
    let balanced_bytes = balanced
        .get("fileBytes")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX);
    assert!(
        balanced_bytes * 4 < wide_bytes.len() as u64,
        "the core did not recode a picture named `Image`, so naming one \
         buys the app nothing: {balanced_bytes} bytes against {} sent: \
         {balanced:?}",
        wide_bytes.len()
    );
    // Core renames image to dated name. JPEG by then.
    let recoded_name = balanced
        .get("fileName")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        recoded_name.starts_with("image_")
            && std::path::Path::new(recoded_name)
                .extension()
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case("jpg")),
        "a recoded PNG is a JPEG under a name of the core's own, and the \
         app shows whichever name the core kept: {balanced:?}"
    );

    let worse = as_image("1").await;
    let worse_bytes = worse
        .get("fileBytes")
        .and_then(Value::as_u64)
        .unwrap_or(u64::MAX);
    assert!(
        worse_bytes < balanced_bytes,
        "media_quality made no difference to what was sent -- {worse_bytes} \
         bytes at the worse setting against {balanced_bytes} at the \
         balanced one -- so the settings page would be offering a choice \
         that does nothing: {worse:?}"
    );
    client
        .call::<_, ()>("set_config", (sender_id, "media_quality", Some("0")))
        .await
        .expect("put media_quality back");
    let _ = std::fs::remove_file(&wide);

    // Largest attachment for relay; conversation refuses bigger.
    let recommended: Option<String> = client
        .call("get_config", (sender_id, "sys.msgsize_max_recommended"))
        .await
        .expect("get_config sys.msgsize_max_recommended");
    assert!(
        recommended
            .as_deref()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|bytes| bytes > 1024 * 1024),
        "the core no longer says how big an attachment it recommends, so \
         the conversation has no ceiling to hold a video to: \
         {recommended:?}"
    );

    // Core leaves GIF dimensions and sound duration empty: don't size from them.
    let animation = std::env::temp_dir().join("piirit-real-server-dot.gif");
    std::fs::write(&animation, ONE_PIXEL_GIF).expect("write gif");
    let sent_gif = send_file(&client, sender_id, saved, &animation, "dot.gif").await;
    assert_eq!(
        sent_gif.get("viewType").and_then(Value::as_str),
        Some("Gif"),
        "the core did not classify a GIF as an animation: {sent_gif:?}"
    );
    for field in ["dimensionsWidth", "dimensionsHeight"] {
        assert_eq!(
            sent_gif.get(field).and_then(Value::as_i64),
            Some(0),
            "the core now reports {field} for a GIF. That is an improvement, \
             but AttachmentPreview's fallback was written because it did \
             not: check the fallback still has a case, then relax this: \
             {sent_gif:?}"
        );
    }

    let tone = std::env::temp_dir().join("piirit-real-server-tone.wav");
    std::fs::write(&tone, one_second_wav()).expect("write wav");
    let recording = std::env::temp_dir().join("piirit-real-server-voice.mp3");
    std::fs::write(&recording, one_second_mp3()).expect("write mp3");

    // Voice message via `send_msg` naming view type; core would call it Audio.
    // MP3 as recorder makes (piirit-shim/src/voice.rs).
    let voice_id: u32 = client
        .call(
            "send_msg",
            (
                sender_id,
                saved,
                serde_json::json!({
                    "file": recording.to_string_lossy(),
                    "viewtype": "Voice",
                    "quotedMessageId": Option::<u32>::None,
                }),
            ),
        )
        .await
        .expect("send_msg with a Voice view type");
    let voices: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (sender_id, vec![voice_id]))
        .await
        .expect("get_messages for the voice message");
    assert_eq!(
        voices[&voice_id].get("viewType").and_then(Value::as_str),
        Some("Voice"),
        "the core did not keep the Voice view type send_msg asked for: {:?}",
        voices[&voice_id]
    );
    // Clients decide by MIME from suffix. iOS shows `audio/ogg` as file.
    assert_eq!(
        voices[&voice_id].get("fileMime").and_then(Value::as_str),
        Some("audio/mpeg"),
        "a recording goes out as something other than MP3: {:?}",
        voices[&voice_id]
    );
    assert!(
        voices[&voice_id]
            .get("file")
            .and_then(Value::as_str)
            .is_some_and(|file| !file.is_empty()),
        "the voice message lost its file: {:?}",
        voices[&voice_id]
    );

    let sent_tone = send_file(&client, sender_id, saved, &tone, "tone.wav").await;
    assert_eq!(
        sent_tone.get("viewType").and_then(Value::as_str),
        Some("Audio"),
        "the core did not classify a WAV as audio: {sent_tone:?}"
    );
    assert_eq!(
        sent_tone.get("fileMime").and_then(Value::as_str),
        Some("audio/wav"),
        "the core reported no MIME type, which the video thumbnailer needs \
         and the file row names itself by: {sent_tone:?}"
    );
    assert_eq!(
        sent_tone.get("fileBytes").and_then(Value::as_u64),
        Some(8044),
        "the core reported the wrong size for a file it just copied: {sent_tone:?}"
    );
    assert_eq!(
        sent_tone.get("duration").and_then(Value::as_i64),
        Some(0),
        "the core now reports a duration for a file it did not before. It \
         carries whatever the sender put in the message rather than probing, \
         which is why nothing renders this field: {sent_tone:?}"
    );

    // Media index (chat_media.rs): up to three view types. Oldest first.
    let tone_id = sent_tone
        .get("id")
        .and_then(Value::as_u64)
        .and_then(|id| u32::try_from(id).ok())
        .expect("the tone's message id");
    let sounds: Vec<u32> = client
        .call(
            "get_chat_media",
            (
                sender_id,
                Some(saved),
                "Audio",
                Some("Voice"),
                Option::<&str>::None,
            ),
        )
        .await
        .expect("get_chat_media for the sounds in a chat");
    assert_eq!(
        sounds,
        vec![voice_id, tone_id],
        "the chat's audio index does not hold the voice message and the tone, \
         oldest first"
    );
    let pictures: Vec<u32> = client
        .call(
            "get_chat_media",
            (sender_id, Some(saved), "Image", Some("Gif"), Some("Video")),
        )
        .await
        .expect("get_chat_media for the pictures in a chat");
    assert!(
        !pictures.contains(&tone_id) && !pictures.contains(&voice_id),
        "a sound is listed among the chat's pictures: {pictures:?}"
    );
    // Calls: own view type in index. None here.
    let calls: Vec<u32> = client
        .call(
            "get_chat_media",
            (
                sender_id,
                Some(saved),
                "Call",
                Option::<&str>::None,
                Option::<&str>::None,
            ),
        )
        .await
        .expect("get_chat_media for the calls in a chat");
    assert!(
        calls.is_empty(),
        "a chat with no calls lists some: {calls:?}"
    );

    // Shared contact: core parses vCard, gives row pieces.
    let ada: u32 = client
        .call(
            "create_contact",
            (sender_id, "ada@example.org", "Ada Lovelace"),
        )
        .await
        .expect("create_contact");
    let card: String = client
        .call("make_vcard", (sender_id, vec![ada]))
        .await
        .expect("make_vcard");
    let card_path = std::env::temp_dir().join("piirit-real-server-ada.vcf");
    std::fs::write(&card_path, &card).expect("write vcard");
    let sent_card = send_file(&client, sender_id, saved, &card_path, "ada.vcf").await;
    assert_eq!(
        sent_card.get("viewType").and_then(Value::as_str),
        Some("Vcard"),
        "the core did not classify a .vcf as a contact: {sent_card:?}"
    );
    for field in ["displayName", "addr", "color"] {
        assert!(
            sent_card
                .pointer(&format!("/vcardContact/{field}"))
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            "the parsed contact has no {field}, which the contact row is \
             built from: {sent_card:?}"
        );
    }

    // Webxdc app: four calls, served via `get_webxdc_blob` (webxdc_host.rs).
    let app_file = std::env::temp_dir().join("piirit-real-server-checkers.xdc");
    let page = b"<html><head></head><body>board</body></html>";
    std::fs::write(
        &app_file,
        webxdc(&[
            ("index.html", page),
            (
                "manifest.toml",
                b"name = \"Checkers\"\nsource_code_url = \"https://example.org/checkers\"\n",
            ),
            ("icon.png", ONE_PIXEL_PNG),
        ]),
    )
    .expect("write webxdc");
    // Sent as plain file, as attach tray does; core turns .xdc into app.
    let (app_id, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                sender_id,
                saved,
                Option::<String>::None,
                Some(app_file.to_string_lossy().into_owned()),
                Some("checkers.xdc".to_string()),
                Option::<(f64, f64)>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .expect("misc_send_msg with a webxdc");
    let apps: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (sender_id, vec![app_id]))
        .await
        .expect("get_messages for the webxdc");
    let sent_app = &apps[&app_id];
    assert_eq!(
        sent_app.get("viewType").and_then(Value::as_str),
        Some("Webxdc"),
        "the core did not turn a .xdc sent as a file into an app, so the \
         attach tray would send a paperclip: {sent_app:?}"
    );

    // Row draw and app self-info.
    let app_info: Value = client
        .call("get_webxdc_info", (sender_id, app_id))
        .await
        .expect("get_webxdc_info");
    assert_eq!(
        app_info.get("name").and_then(Value::as_str),
        Some("Checkers"),
        "the core did not read the app's name out of its manifest: {app_info:?}"
    );
    assert_eq!(
        app_info.get("icon").and_then(Value::as_str),
        Some("icon.png"),
        "the icon is not named as a path inside the archive, which is the \
         only way to ask for it: {app_info:?}"
    );
    assert_eq!(
        app_info.get("sourceCodeUrl").and_then(Value::as_str),
        Some("https://example.org/checkers"),
        "unexpected source-code field: {app_info:?}"
    );
    assert_eq!(
        app_info.get("internetAccess").and_then(Value::as_bool),
        Some(false),
        "an ordinary app is being granted the internet: {app_info:?}"
    );
    assert!(
        app_info
            .get("selfAddr")
            .and_then(Value::as_str)
            .is_some_and(|addr| !addr.is_empty()),
        "no selfAddr, which is what the app sees as its own identity: {app_info:?}"
    );
    for field in ["sendUpdateInterval", "sendUpdateMaxSize"] {
        assert!(
            app_info.get(field).and_then(Value::as_u64).is_some(),
            "no {field}, which the app is handed as its own limit: {app_info:?}"
        );
    }

    // Files as host serves: base64 without padding.
    let encoded_page: String = client
        .call("get_webxdc_blob", (sender_id, app_id, "index.html"))
        .await
        .expect("get_webxdc_blob");
    assert_eq!(
        encoded_page,
        base64(page),
        "the core encodes a blob differently than the host decodes it"
    );
    let encoded_icon: String = client
        .call("get_webxdc_blob", (sender_id, app_id, "icon.png"))
        .await
        .expect("get_webxdc_blob for the icon");
    assert_eq!(
        encoded_icon,
        base64(ONE_PIXEL_PNG),
        "the icon a row draws is not the one in the archive"
    );

    // Updates both ways: bridge POST, and poll collecting it.
    let update = "{\"payload\":{\"move\":\"e4\"}}";
    client
        .call::<_, Value>(
            "send_webxdc_status_update",
            (sender_id, app_id, update, Option::<String>::None),
        )
        .await
        .expect("send_webxdc_status_update");
    let sent_updates: String = client
        .call("get_webxdc_status_updates", (sender_id, app_id, 0))
        .await
        .expect("get_webxdc_status_updates");
    // JSON string, not array: host hands it unread.
    let collected: Value =
        serde_json::from_str(&sent_updates).expect("the updates are a JSON array");
    assert_eq!(
        collected.pointer("/0/payload/move").and_then(Value::as_str),
        Some("e4"),
        "the update did not come back as it was sent: {collected:?}"
    );
    assert!(
        collected
            .pointer("/0/serial")
            .and_then(Value::as_u64)
            .is_some_and(|serial| serial > 0),
        "an update came back without a serial, which is what the app asks \
         for the next one from: {collected:?}"
    );
    assert!(
        collected.pointer("/0/max_serial").is_some(),
        "no max_serial, which the webxdc API passes to the app as it is \
         spelled here: {collected:?}"
    );

    // Chat kind decides if message names sender.
    let group: u32 = client
        .call("create_group_chat", (sender_id, "Shape Group", false))
        .await
        .expect("create_group_chat");
    let info: Value = client
        .call("get_basic_chat_info", (sender_id, group))
        .await
        .expect("get_basic_chat_info");
    assert_eq!(
        info.get("chatType").and_then(Value::as_str),
        Some("Group"),
        "unexpected chat info shape: {info:?}"
    );

    // Chat-list row as list consumes it: every field, core's spelling.
    let saved_items: std::collections::HashMap<u32, Value> = client
        .call("get_chatlist_items_by_entries", (sender_id, vec![saved]))
        .await
        .expect("get_chatlist_items_by_entries for the saved chat");
    let row = &saved_items[&saved];
    for field in [
        "freshMessageCounter",
        "lastUpdated",
        "summaryText1",
        "summaryText2",
        "summaryStatus",
        "isPinned",
        "isMuted",
        "isContactRequest",
        "isSelfTalk",
        "isDeviceTalk",
        "color",
        "avatarPath",
    ] {
        assert!(
            row.get(field).is_some(),
            "the chat list row lost the {field} field: {row:?}"
        );
    }
    // Core 2.63 dropped thumbnail from row. List never drew it.
    assert!(
        row.get("summaryPreviewImage").is_none(),
        "the chat list row has a summaryPreviewImage again: {row:?}"
    );
    // Self-chat flagged, so cover skips it.
    assert_eq!(
        row.get("isSelfTalk").and_then(Value::as_bool),
        Some(true),
        "the saved-messages chat is not marked as the chat with oneself: {row:?}"
    );
    let group_items: std::collections::HashMap<u32, Value> = client
        .call("get_chatlist_items_by_entries", (sender_id, vec![group]))
        .await
        .expect("get_chatlist_items_by_entries for the group");
    assert_eq!(
        group_items[&group]
            .get("isSelfTalk")
            .and_then(Value::as_bool),
        Some(false),
        "a group reads as the chat with oneself: {:?}",
        group_items[&group]
    );

    // get_contacts: listFlags 0 omits self; DC_GCL_ADD_SELF (2) adds it.
    let contacts_plain: Vec<Value> = client
        .call("get_contacts", (sender_id, 0, Option::<String>::None))
        .await
        .expect("get_contacts");
    assert!(
        !contacts_plain
            .iter()
            .any(|contact| contact.get("id").and_then(Value::as_u64) == Some(1)),
        "get_contacts lists the account's own contact without being asked: {contacts_plain:?}"
    );
    let contacts_with_self: Vec<Value> = client
        .call("get_contacts", (sender_id, 2, Option::<String>::None))
        .await
        .expect("get_contacts with DC_GCL_ADD_SELF");
    assert_eq!(
        contacts_with_self
            .last()
            .and_then(|contact| contact.get("id"))
            .and_then(Value::as_u64),
        Some(1),
        "DC_GCL_ADD_SELF does not put the account's own contact last: {contacts_with_self:?}"
    );
    let contacts_searched: Vec<Value> = client
        .call("get_contacts", (sender_id, 2, Some("zzz-nobody")))
        .await
        .expect("get_contacts with a query");
    assert!(
        contacts_searched.is_empty(),
        "a query still lists the account's own contact: {contacts_searched:?}"
    );

    // Blocking: `get_blocked_contacts` gives whole contact; each change announces
    // `ContactsChanged`.
    //
    // Blocked contact leaving `get_contacts` not asserted: offline session makes address
    // contacts only. Fake core and `piirit-shim/tests/blocking.rs` cover it.
    client
        .call::<_, ()>("block_contact", (sender_id, ada))
        .await
        .expect("block_contact");
    let blocked: Vec<Value> = client
        .call("get_blocked_contacts", (sender_id,))
        .await
        .expect("get_blocked_contacts");
    let blocked_ada = blocked
        .iter()
        .find(|contact| contact.get("id").and_then(Value::as_u64) == Some(u64::from(ada)))
        .unwrap_or_else(|| panic!("a blocked contact is not on the blocked list: {blocked:?}"));
    assert_eq!(
        (
            blocked_ada.get("isBlocked").and_then(Value::as_bool),
            blocked_ada.get("displayName").and_then(Value::as_str),
        ),
        (Some(true), Some("Ada Lovelace")),
        "the blocked list does not carry whole contacts: {blocked_ada:?}"
    );
    let mut saw_contacts_changed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(event)) => {
                if event.context_id == sender_id
                    && event.event.get("kind").and_then(Value::as_str) == Some("ContactsChanged")
                {
                    saw_contacts_changed = true;
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_contacts_changed,
        "no ContactsChanged event arrived after blocking a contact, so an \
         open block list would never hear of one made elsewhere"
    );
    client
        .call::<_, ()>("unblock_contact", (sender_id, ada))
        .await
        .expect("unblock_contact");
    let blocked_after: Vec<Value> = client
        .call("get_blocked_contacts", (sender_id,))
        .await
        .expect("get_blocked_contacts after unblocking");
    assert!(
        !blocked_after
            .iter()
            .any(|contact| contact.get("id").and_then(Value::as_u64) == Some(u64::from(ada))),
        "an unblocked contact is still on the blocked list: {blocked_after:?}"
    );

    // Row menu: visibility is one method; mute takes tagged duration.
    for visibility in ["Pinned", "Archived", "Normal"] {
        client
            .call::<_, ()>("set_chat_visibility", (sender_id, group, visibility))
            .await
            .unwrap_or_else(|err| panic!("set_chat_visibility {visibility}: {err}"));
    }
    for kind in ["Forever", "NotMuted"] {
        client
            .call::<_, ()>(
                "set_chat_mute_duration",
                (sender_id, group, serde_json::json!({"kind": kind})),
            )
            .await
            .unwrap_or_else(|err| panic!("set_chat_mute_duration {kind}: {err}"));
    }
    client
        .call::<_, ()>("delete_chat", (sender_id, group))
        .await
        .expect("delete_chat");

    // Message menu: both take id list. Reply is misc_send_msg, quote last.
    client
        .call::<_, ()>("resend_messages", (sender_id, vec![first]))
        .await
        .expect("resend_messages");
    client
        .call::<_, ()>("delete_messages", (sender_id, vec![second]))
        .await
        .expect("delete_messages");

    // Read receipts, as ChatMessages sends on open.
    client
        .call::<_, ()>("markseen_msgs", (sender_id, vec![first]))
        .await
        .expect("markseen_msgs");

    // Reactions: one call, answers hidden reaction id. Counts sorted; null once emptied.
    let listed_before: Vec<Value> = client
        .call("get_message_list_items", (sender_id, saved, false, false))
        .await
        .expect("get_message_list_items before reacting");
    let carrier: u32 = client
        .call("send_reaction", (sender_id, first, vec!["👍"]))
        .await
        .expect("send_reaction");
    assert_ne!(
        carrier, first,
        "the reaction's carrier is the message itself"
    );
    let reacted: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (sender_id, vec![first]))
        .await
        .expect("get_messages after reacting");
    let reactions = reacted[&first]
        .pointer("/reactions/reactions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert_eq!(
        reactions.len(),
        1,
        "one reaction did not come back as one: {:?}",
        reacted[&first]
    );
    assert_eq!(
        reactions[0].get("emoji").and_then(Value::as_str),
        Some("👍"),
        "the reaction is not the emoji that was sent: {reactions:?}"
    );
    assert_eq!(
        reactions[0].get("count").and_then(Value::as_u64),
        Some(1),
        "the reaction is not counted: {reactions:?}"
    );
    assert_eq!(
        reactions[0].get("isFromSelf").and_then(Value::as_bool),
        Some(true),
        "our own reaction is not marked as ours, so a second tap cannot \
         take it off: {reactions:?}"
    );
    let listed_after: Vec<Value> = client
        .call("get_message_list_items", (sender_id, saved, false, false))
        .await
        .expect("get_message_list_items after reacting");
    assert_eq!(
        listed_before.len(),
        listed_after.len(),
        "a reaction shows up as a row of its own: {listed_after:?}"
    );
    // Event model refreshes row on, with message named.
    let mut saw_reactions_changed = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, events.recv()).await {
            Ok(Some(event)) => {
                if event.context_id == sender_id
                    && event.event.get("kind").and_then(Value::as_str) == Some("ReactionsChanged")
                    && event.event.get("msgId").and_then(Value::as_u64) == Some(u64::from(first))
                {
                    saw_reactions_changed = true;
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        saw_reactions_changed,
        "no ReactionsChanged event named message {first} after reacting to it"
    );
    let _: u32 = client
        .call("send_reaction", (sender_id, first, Vec::<String>::new()))
        .await
        .expect("send_reaction with an empty list");
    let cleared: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (sender_id, vec![first]))
        .await
        .expect("get_messages after clearing the reaction");
    assert!(
        cleared[&first].get("reactions").is_some_and(Value::is_null),
        "an empty list did not take the reaction off: {:?}",
        cleared[&first]
    );

    // Downstream UI relies on these; wrong method or key fails only on device.

    // `isForwarded`: true on copy (sender and recipient), false on original. Copy alone would pass
    // an always-true field.
    let (original, _): (u32, Value) = client
        .call(
            "misc_send_msg",
            (
                sender_id,
                saved,
                Some("to forward"),
                Option::<String>::None,
                Option::<String>::None,
                Option::<(f64, f64)>::None,
                Option::<u32>::None,
            ),
        )
        .await
        .expect("misc_send_msg for the forwarding probe");
    client
        .call::<_, ()>("forward_messages", (sender_id, vec![original], saved))
        .await
        .expect("forward_messages");
    let listed: Vec<Value> = client
        .call("get_message_list_items", (sender_id, saved, false, false))
        .await
        .expect("get_message_list_items after forwarding");
    let ids: Vec<u32> = listed
        .iter()
        .filter_map(|item| item.get("msg_id").and_then(Value::as_u64))
        .filter_map(|id| u32::try_from(id).ok())
        .collect();
    let after: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (sender_id, ids))
        .await
        .expect("get_messages after forwarding");
    let forwarded_flags: Vec<bool> = after
        .values()
        .filter_map(|message| message.get("isForwarded").and_then(Value::as_bool))
        .collect();
    assert!(
        forwarded_flags.iter().any(|flag| *flag),
        "no message reports isForwarded after forward_messages: {after:?}"
    );
    assert!(
        forwarded_flags.iter().any(|flag| !*flag),
        "every message reports isForwarded, so the field marks nothing: {after:?}"
    );

    // Search: three args, last optional chat; two rejected.
    let _: Vec<u32> = client
        .call(
            "search_messages",
            (sender_id, "forward", Option::<u32>::None),
        )
        .await
        .expect("search_messages takes (account, query, chat_id option)");

    // Profile fields a settings page edits; two not covered above.
    client
        .call::<_, ()>("set_config", (sender_id, "selfstatus", "probing"))
        .await
        .expect("set_config selfstatus");
    let status: Option<String> = client
        .call("get_config", (sender_id, "selfstatus"))
        .await
        .expect("get_config selfstatus");
    assert_eq!(
        status.as_deref(),
        Some("probing"),
        "selfstatus did not stick"
    );

    // `selfavatar` is a path to an image core copies. Empty string rejected.
    let avatar = std::env::temp_dir().join("piirit-real-server-avatar.png");
    // Smallest valid PNG; core rejects undecodable.
    std::fs::write(
        &avatar,
        [
            137u8, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 96, 0, 2,
            0, 0, 5, 0, 1, 122, 94, 171, 63, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ],
    )
    .expect("write probe avatar");
    client
        .call::<_, ()>(
            "set_config",
            (
                sender_id,
                "selfavatar",
                Some(avatar.to_string_lossy().into_owned()),
            ),
        )
        .await
        .expect("set_config selfavatar with a real image path");
    let stored: Option<String> = client
        .call("get_config", (sender_id, "selfavatar"))
        .await
        .expect("get_config selfavatar");
    assert!(
        stored.as_deref().is_some_and(|path| !path.is_empty()),
        "selfavatar read back empty after being set to an image: {stored:?}"
    );
    // Null clears: the "remove picture" action.
    client
        .call::<_, ()>(
            "set_config",
            (sender_id, "selfavatar", Option::<String>::None),
        )
        .await
        .expect("set_config selfavatar null clears it");
    let _ = std::fs::remove_file(&avatar);

    // Configured account: name, picture, colour; no address (core 2.61 removed).
    let accounts: Vec<Value> = client
        .call_unit("get_all_accounts")
        .await
        .expect("get_all_accounts after configuring one");
    let configured = accounts
        .iter()
        .find(|a| a.get("id").and_then(Value::as_u64) == Some(u64::from(sender_id)))
        .expect("the configured account is missing from get_all_accounts");
    assert_eq!(
        configured.get("kind").and_then(Value::as_str),
        Some("Configured"),
        "unexpected account shape: {configured:?}"
    );
    for field in ["displayName", "profileImage", "color"] {
        assert!(
            configured.get(field).is_some(),
            "a configured account lost the {field} field, which the profiles \
             page draws: {configured:?}"
        );
    }
    let own_addr: Option<String> = client
        .call("get_config", (sender_id, "configured_addr"))
        .await
        .expect("get_config configured_addr");
    assert_eq!(
        own_addr.as_deref(),
        Some("self@example.invalid"),
        "a configured account does not name the address it was configured \
         with, so its row on the profiles page would have none"
    );

    // Empty name resets a contact's own name; contact page relies on it.
    client
        .call::<_, ()>("change_contact_name", (sender_id, ada, "Countess"))
        .await
        .expect("change_contact_name");
    let named: std::collections::HashMap<u32, Value> = client
        .call("get_contacts_by_ids", (sender_id, vec![ada]))
        .await
        .expect("get_contacts_by_ids after the rename");
    assert_eq!(
        named[&ada].get("displayName").and_then(Value::as_str),
        Some("Countess"),
        "the given name is not what the contact is shown as: {named:?}"
    );
    for field in ["name", "authName"] {
        assert!(
            named[&ada].get(field).is_some(),
            "the contact lost the {field} field, which the name field is \
             built from: {named:?}"
        );
    }
    client
        .call::<_, ()>("change_contact_name", (sender_id, ada, ""))
        .await
        .expect("change_contact_name with an empty name");
    let unnamed: std::collections::HashMap<u32, Value> = client
        .call("get_contacts_by_ids", (sender_id, vec![ada]))
        .await
        .expect("get_contacts_by_ids after blanking the name");
    assert_eq!(
        unnamed[&ada].get("name").and_then(Value::as_str),
        Some(""),
        "an empty name did not clear the given one: {unnamed:?}"
    );
    assert_ne!(
        unnamed[&ada].get("displayName").and_then(Value::as_str),
        Some("Countess"),
        "the contact is still shown under the cleared name: {unnamed:?}"
    );

    // Connection/mailbox calls: pinned as shapes connectivity.rs reads.
    let band: u32 = client
        .call("get_connectivity", (sender_id,))
        .await
        .expect("get_connectivity");
    assert!(
        band >= 1000,
        "the connectivity band is below the core's own scale: {band}"
    );
    let report: String = client
        .call("get_connectivity_html", (sender_id,))
        .await
        .expect("get_connectivity_html");
    assert!(
        report.contains("<h3>"),
        "the connectivity report is not the page the core writes: {report:?}"
    );
    let size: u64 = client
        .call("get_account_file_size", (sender_id,))
        .await
        .expect("get_account_file_size");
    assert!(
        size > 0,
        "an account with messages in it takes no space: {size}"
    );

    // Download limit: per account, config key in bytes, read back as written.
    client
        .call::<_, ()>("set_config", (sender_id, "download_limit", "1048576"))
        .await
        .expect("set_config download_limit");
    let limit: Option<String> = client
        .call("get_config", (sender_id, "download_limit"))
        .await
        .expect("get_config download_limit");
    assert_eq!(
        limit.as_deref(),
        Some("1048576"),
        "download_limit did not stick"
    );
    // Mark/count unread: calls exist and answer shapes chatlist.rs and core.rs read.
    client
        .call::<_, ()>("markfresh_chat", (sender_id, saved))
        .await
        .expect("markfresh_chat");
    let fresh: Vec<u32> = client
        .call("get_fresh_msgs", (sender_id,))
        .await
        .expect("get_fresh_msgs");
    let fresh_here: usize = client
        .call("get_fresh_msg_cnt", (sender_id, saved))
        .await
        .expect("get_fresh_msg_cnt");
    assert!(
        fresh.is_empty() && fresh_here == 0,
        "a chat of the reader's own messages counts as unread: {fresh:?}, {fresh_here}"
    );

    // `downloadState` tells held-back messages apart; this one isn't (Done). Delegate offers the
    // rest on anything but Done.
    assert_eq!(
        reply.get("downloadState").and_then(Value::as_str),
        Some("Done"),
        "a message sent from here is not marked as fully there: {reply:?}"
    );

    unpinning_against(&client, sender_id, saved, first).await;
    relays_against(&client, mailbox).await;
    calls_against(&client, mailbox).await;
    channels_against(&client, mailbox).await;

    let _ = std::fs::remove_file(&attachment);
    handle.stop();
    client.shutdown().await.expect("shutdown");
    let _ = std::fs::remove_dir_all(&accounts_dir);
}

/// Pinning a message, then unpinning it, as another Delta Chat app does in a chat
/// Piirit shows. Core 2.65+ leaves a visible info row for the unpin too, where it
/// used to hide it; Piirit draws it as any other info row.
async fn unpinning_against(client: &RpcClient, account: u32, chat: u32, message: u32) {
    let last_row = move || async move {
        let ids: Vec<u32> = client
            .call("get_message_ids", (account, chat, false, false))
            .await
            .unwrap_or_else(|err| panic!("get_message_ids: {err}"));
        let Some(&last) = ids.last() else {
            panic!("the chat has no messages")
        };
        let rows: std::collections::HashMap<u32, Value> = client
            .call("get_messages", (account, vec![last]))
            .await
            .unwrap_or_else(|err| panic!("get_messages: {err}"));
        rows[&last].clone()
    };
    for (state, kind, text) in [
        (true, "MessagePinned", "You pinned a message."),
        (false, "MessageUnpinned", "You unpinned a message."),
    ] {
        client
            .call::<_, ()>("set_pinned_message_state", (account, message, state))
            .await
            .unwrap_or_else(|err| panic!("set_pinned_message_state {state}: {err}"));
        let pinned: Vec<u32> = client
            .call("get_pinned_messages", (account, chat))
            .await
            .unwrap_or_else(|err| panic!("get_pinned_messages: {err}"));
        assert_eq!(pinned.contains(&message), state, "pinned now: {pinned:?}");
        let row = last_row().await;
        assert_eq!(
            row.get("systemMessageType").and_then(Value::as_str),
            Some(kind),
            "the chat does not end in the {kind} info row: {row:?}"
        );
        assert_eq!(
            row.get("isInfo").and_then(Value::as_bool),
            Some(true),
            "{row:?}"
        );
        assert_eq!(
            row.get("text").and_then(Value::as_str),
            Some(text),
            "{row:?}"
        );
    }
}

/// Profile relays, as profile page and duplicate check read them. Three loopback relays.
/// `relays_against` adds them.
async fn relays_listed_as_added(client: &RpcClient, account: u32, addrs: &[&str]) {
    // Relays by address, in add order; profile's own address stays on first.
    let listed = |transports: &[Value]| -> Vec<String> {
        transports
            .iter()
            .map(|transport| {
                transport
                    .get("addr")
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| panic!("a transport without an addr: {transport:?}"))
                    .to_string()
            })
            .collect()
    };
    let transports: Vec<Value> = client
        .call("list_transports", (account,))
        .await
        .unwrap_or_else(|err| panic!("list_transports: {err}"));
    assert_eq!(listed(&transports), addrs, "{transports:?}");
    let own: Option<String> = client
        .call("get_config", (account, "configured_addr"))
        .await
        .unwrap_or_else(|err| panic!("get_config configured_addr: {err}"));
    assert_eq!(own.as_deref(), Some(addrs[0]));
    // Core 2.63+: relays don't write legacy keys (`addr`, `mail_pw`); shim must never read them.
    for key in ["addr", "mail_pw", "mail_server", "send_server"] {
        let legacy: Option<String> = client
            .call("get_config", (account, key))
            .await
            .unwrap_or_else(|err| panic!("get_config {key}: {err}"));
        assert_eq!(
            legacy.as_deref().filter(|value| !value.is_empty()),
            None,
            "adding a relay wrote the legacy {key} config"
        );
    }
}

async fn relays_against(client: &RpcClient, mailbox: (u16, u16)) {
    let account: u32 = client
        .call_unit("add_account")
        .await
        .unwrap_or_else(|err| panic!("add_account for the relays probe: {err}"));
    let addrs = [
        "one@example.invalid",
        "two@example.invalid",
        "three@example.invalid",
    ];
    for addr in addrs {
        client
            .call::<_, ()>("add_transport_from_qr", (account, dclogin(addr, mailbox)))
            .await
            .unwrap_or_else(|err| panic!("add_transport_from_qr {addr}: {err}"));
    }
    client
        .call::<_, ()>("stop_io", (account,))
        .await
        .unwrap_or_else(|err| panic!("stop_io on the relays probe: {err}"));

    relays_listed_as_added(client, account, &addrs).await;

    // Own invite link, as the QR page shows and shares it. Core 2.64+
    // writes '@' as %40 again in every relay address the link names,
    // so apps that end a link at an '@' keep the whole invite.
    let invite: String = client
        .call(
            "get_chat_securejoin_qr_code",
            (account, Option::<u32>::None),
        )
        .await
        .unwrap_or_else(|err| panic!("get_chat_securejoin_qr_code: {err}"));
    for addr in addrs {
        assert!(
            invite.contains(&addr.replace('@', "%40")),
            "the invite link does not name relay {addr} with '@' as %40: {invite}"
        );
    }
    assert!(
        !invite.contains('@'),
        "the invite link has a bare '@': {invite}"
    );

    // Refusals in core's words, as fake core gives them.
    assert_eq!(
        refusal(
            client,
            "set_config",
            (account, "configured_addr", "nobody@example.invalid")
        )
        .await,
        "Address does not belong to any transport."
    );
    assert_eq!(
        refusal(
            client,
            "set_config",
            (account, "configured_addr", Option::<String>::None)
        )
        .await,
        "Cannot unset configured_addr"
    );
    assert_eq!(
        refusal(
            client,
            "delete_transport",
            (account, "nobody@example.invalid")
        )
        .await,
        "Transport does not exist"
    );

    // Removing relay with profile's address moves it to newest remaining.
    client
        .call::<_, ()>("delete_transport", (account, addrs[0]))
        .await
        .unwrap_or_else(|err| panic!("delete_transport of the profile's own relay: {err}"));
    let own: Option<String> = client
        .call("get_config", (account, "configured_addr"))
        .await
        .unwrap_or_else(|err| panic!("get_config configured_addr after the removal: {err}"));
    assert_eq!(
        own.as_deref(),
        Some(addrs[2]),
        "the profile's own address did not move to the newest relay left"
    );
    client
        .call::<_, ()>("delete_transport", (account, addrs[1]))
        .await
        .unwrap_or_else(|err| panic!("delete_transport of a second relay: {err}"));
    assert_eq!(
        refusal(client, "delete_transport", (account, addrs[2])).await,
        "Cannot remove the last transport"
    );
    // Last relay refused before unknown one looked for.
    assert_eq!(
        refusal(
            client,
            "delete_transport",
            (account, "nobody@example.invalid")
        )
        .await,
        "Cannot remove the last transport"
    );
}

/// Call as shim places, reads, ends it (`calls.rs`, `call_host.rs`). No media: SDP is a stand-in.
/// Two accounts on loopback stub; nothing delivered, pins this end.
async fn calls_against(client: &RpcClient, mailbox: (u16, u16)) {
    let mut accounts = Vec::new();
    for addr in ["caller@example.invalid", "callee@example.invalid"] {
        let account: u32 = client
            .call_unit("add_account")
            .await
            .unwrap_or_else(|err| panic!("add_account for the calls probe: {err}"));
        client
            .call::<_, ()>("add_transport_from_qr", (account, dclogin(addr, mailbox)))
            .await
            .unwrap_or_else(|err| panic!("add_transport_from_qr {addr}: {err}"));
        client
            .call::<_, ()>("stop_io", (account,))
            .await
            .unwrap_or_else(|err| panic!("stop_io on the calls probe: {err}"));
        accounts.push(account);
    }
    let (caller, other_end) = (accounts[0], accounts[1]);
    // DC_CONTACT_ID_SELF: other end's contact as vCard with key.
    let card: String = client
        .call("make_vcard", (other_end, vec![1_u32]))
        .await
        .unwrap_or_else(|err| panic!("make_vcard: {err}"));
    let imported: Vec<u32> = client
        .call("import_vcard_contents", (caller, card))
        .await
        .unwrap_or_else(|err| panic!("import_vcard_contents: {err}"));
    let contact = imported[0];
    let chat: u32 = client
        .call("create_chat_by_contact_id", (caller, contact))
        .await
        .unwrap_or_else(|err| panic!("create_chat_by_contact_id: {err}"));
    a_call_is_placed_and_hung_up(client, caller, chat).await;
    calls_are_refused_where_the_core_refuses_them(client, caller, chat).await;
    // Same key contact: only one can join a group.
    chats_in_common_against(client, caller, contact, chat).await;
}

/// Core lists a contact's 1:1 and groups, archived too. Own id lists every chat, so the page
/// asks nothing for saved messages.
async fn chats_in_common_against(client: &RpcClient, account: u32, contact: u32, chat: u32) {
    let shared: u32 = client
        .call("create_group_chat", (account, "Shared Group", false))
        .await
        .unwrap_or_else(|err| panic!("create_group_chat: {err}"));
    client
        .call::<_, ()>("add_contact_to_chat", (account, shared, contact))
        .await
        .unwrap_or_else(|err| panic!("add_contact_to_chat: {err}"));
    let other: u32 = client
        .call("create_group_chat", (account, "Other Group", false))
        .await
        .unwrap_or_else(|err| panic!("create_group_chat: {err}"));
    client
        .call::<_, ()>("set_chat_visibility", (account, chat, "Archived"))
        .await
        .unwrap_or_else(|err| panic!("set_chat_visibility: {err}"));
    let by_contact = |contact: u32| async move {
        client
            .call::<_, Vec<u32>>(
                "get_chatlist_entries",
                (
                    account,
                    Option::<u32>::None,
                    Option::<String>::None,
                    Some(contact),
                ),
            )
            .await
            .unwrap_or_else(|err| panic!("get_chatlist_entries by contact: {err}"))
    };
    let common = by_contact(contact).await;
    assert!(
        common.contains(&chat) && common.contains(&shared) && !common.contains(&other),
        "chats with the contact are not chat {chat} (archived) and group {shared} \
         without group {other}: {common:?}"
    );
    let own = by_contact(1).await;
    assert!(
        own.contains(&other),
        "own contact no longer lists every chat; saved messages could show chats in \
         common: {own:?}"
    );
}

/// Placed, read back, hung up before answer.
async fn a_call_is_placed_and_hung_up(client: &RpcClient, caller: u32, chat: u32) {
    // ICE servers: JSON string inside answer, passed to page as is.
    let ice: String = client
        .call("ice_servers", (caller,))
        .await
        .unwrap_or_else(|err| panic!("ice_servers: {err}"));
    assert!(
        serde_json::from_str::<Vec<Value>>(&ice).is_ok(),
        "ice_servers is not a JSON array in a string: {ice}"
    );

    // Placed: call is a message; its id is for `call_info` and `end_call`.
    let placed: u32 = client
        .call(
            "place_outgoing_call",
            (caller, chat, "v=0 offer from a test", false),
        )
        .await
        .unwrap_or_else(|err| panic!("place_outgoing_call: {err}"));
    let info: Value = client
        .call("call_info", (caller, placed))
        .await
        .unwrap_or_else(|err| panic!("call_info: {err}"));
    assert_eq!(
        info,
        serde_json::json!({
            "sdpOffer": "v=0 offer from a test",
            "hasVideo": false,
            "state": {"kind": "Alerting"},
        }),
        "call_info is not the camelCase object, state tagged by kind, that \
         calls.rs reads"
    );
    let message: Value = client
        .call("get_message", (caller, placed))
        .await
        .unwrap_or_else(|err| panic!("get_message for the call: {err}"));
    assert_eq!(
        message["viewType"],
        serde_json::json!("Call"),
        "{message:?}"
    );
    assert_eq!(message["fromId"], serde_json::json!(1), "{message:?}");

    // Hung up unanswered: canceled; saying so twice is no error.
    client
        .call::<_, Value>("end_call", (caller, placed))
        .await
        .unwrap_or_else(|err| panic!("end_call: {err}"));
    let info: Value = client
        .call("call_info", (caller, placed))
        .await
        .unwrap_or_else(|err| panic!("call_info after end_call: {err}"));
    assert_eq!(
        info["state"],
        serde_json::json!({"kind": "Canceled"}),
        "{info:?}"
    );
    client
        .call::<_, Value>("end_call", (caller, placed))
        .await
        .unwrap_or_else(|err| panic!("end_call on an ended call: {err}"));
}

/// Core's own refusals, and the one setting.
async fn calls_are_refused_where_the_core_refuses_them(client: &RpcClient, caller: u32, chat: u32) {
    let text: u32 = client
        .call("misc_send_text_message", (caller, chat, "not a call"))
        .await
        .unwrap_or_else(|err| panic!("misc_send_text_message: {err}"));
    assert_eq!(
        refusal(client, "call_info", (caller, text)).await,
        format!("Attempting to get call state of non-call message Msg#{text}")
    );
    let saved: u32 = client
        .call("create_chat_by_contact_id", (caller, 1_u32))
        .await
        .unwrap_or_else(|err| panic!("create_chat_by_contact_id for Saved messages: {err}"));
    assert_eq!(
        refusal(client, "place_outgoing_call", (caller, saved, "v=0", false)).await,
        "Cannot call self"
    );
    let group: u32 = client
        .call("create_group_chat", (caller, "Calls", true))
        .await
        .unwrap_or_else(|err| panic!("create_group_chat: {err}"));
    assert_eq!(
        refusal(client, "place_outgoing_call", (caller, group, "v=0", false)).await,
        "Can only place calls in single chats"
    );

    // Whose calls ring here: settings value.
    client
        .call::<_, ()>("set_config", (caller, "who_can_call_me", "2"))
        .await
        .unwrap_or_else(|err| panic!("set_config who_can_call_me: {err}"));
    let who: Option<String> = client
        .call("get_config", (caller, "who_can_call_me"))
        .await
        .unwrap_or_else(|err| panic!("get_config who_can_call_me: {err}"));
    assert_eq!(who.as_deref(), Some("2"));
}

/// Channel made as New channel page makes it (`ContactList::create_channel`), joined as QR page
/// follows its invite (`join_by_invite`): owner's chat, `check_qr` kind, read-only reader's chat.
/// Nothing delivered: pins both ends' calls.
async fn channels_against(client: &RpcClient, mailbox: (u16, u16)) {
    let mut accounts = Vec::new();
    for addr in ["owner@example.invalid", "reader@example.invalid"] {
        let account: u32 = client
            .call_unit("add_account")
            .await
            .unwrap_or_else(|err| panic!("add_account for the channel probe: {err}"));
        client
            .call::<_, ()>("add_transport_from_qr", (account, dclogin(addr, mailbox)))
            .await
            .unwrap_or_else(|err| panic!("add_transport_from_qr {addr}: {err}"));
        client
            .call::<_, ()>("stop_io", (account,))
            .await
            .unwrap_or_else(|err| panic!("stop_io on the channel probe: {err}"));
        accounts.push(account);
    }
    let (owner, reader) = (accounts[0], accounts[1]);
    let channel: u32 = client
        .call("create_broadcast", (owner, "News"))
        .await
        .unwrap_or_else(|err| panic!("create_broadcast: {err}"));
    let own: Value = client
        .call("get_full_chat_by_id", (owner, channel))
        .await
        .unwrap_or_else(|err| panic!("get_full_chat_by_id of the new channel: {err}"));
    // Channel page tells own channel (offers its code) by this.
    assert_eq!(
        own.get("chatType").and_then(Value::as_str),
        Some("OutBroadcast"),
        "create_broadcast did not make an own channel: {own:?}"
    );
    assert_eq!(
        own.get("name").and_then(Value::as_str),
        Some("News"),
        "the channel is not named as typed: {own:?}"
    );
    assert_eq!(
        own.get("canSend").and_then(Value::as_bool),
        Some(true),
        "the owner cannot post to their channel: {own:?}"
    );
    // No members by hand: why page offers none, code is only way in.
    let card: String = client
        .call("make_vcard", (reader, vec![1_u32]))
        .await
        .unwrap_or_else(|err| panic!("make_vcard: {err}"));
    let imported: Vec<u32> = client
        .call("import_vcard_contents", (owner, card))
        .await
        .unwrap_or_else(|err| panic!("import_vcard_contents: {err}"));
    assert!(
        refusal(client, "add_contact_to_chat", (owner, channel, imported[0]))
            .await
            .contains("not a group where one can add members"),
        "the core took a subscriber added by hand"
    );
    let invite: String = client
        .call("get_chat_securejoin_qr_code", (owner, Some(channel)))
        .await
        .unwrap_or_else(|err| panic!("get_chat_securejoin_qr_code for the channel: {err}"));

    let qr: Value = client
        .call("check_qr", (reader, invite.as_str()))
        .await
        .unwrap_or_else(|err| panic!("check_qr of the channel invite: {err}"));
    assert_eq!(
        qr.get("kind").and_then(Value::as_str),
        Some("askJoinBroadcast"),
        "a channel invite is not the kind join_by_invite follows: {qr:?}"
    );
    assert_eq!(
        qr.get("name").and_then(Value::as_str),
        Some("News"),
        "unexpected channel invite shape: {qr:?}"
    );

    let joined: u32 = client
        .call("secure_join", (reader, invite.as_str()))
        .await
        .unwrap_or_else(|err| panic!("secure_join of the channel invite: {err}"));
    let chat: Value = client
        .call("get_full_chat_by_id", (reader, joined))
        .await
        .unwrap_or_else(|err| panic!("get_full_chat_by_id of the joined channel: {err}"));
    assert_eq!(
        chat.get("chatType").and_then(Value::as_str),
        Some("InBroadcast"),
        "secure_join did not open the channel: {chat:?}"
    );
    assert_eq!(
        chat.get("name").and_then(Value::as_str),
        Some("News"),
        "the joined channel is not named as its invite: {chat:?}"
    );
    // Subscriber reads only; conversation page offers no composer.
    assert_eq!(
        chat.get("canSend").and_then(Value::as_bool),
        Some(false),
        "a subscriber can write to the channel: {chat:?}"
    );
}
