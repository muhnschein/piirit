//! Integration test against a REAL `deltachat-rpc-server` binary -- the
//! Milestone 1 acceptance check (`get_system_info` over stdio) plus enough
//! offline account/chat operations to prove the transport's assumptions
//! hold against the actual core, not just the in-repo fake.
//!
//! Gated: skipped unless `DELTACHAT_RPC_SERVER` points at a real binary
//! (e.g. one extracted from upstream's `PyPI` wheel or GitHub release), so
//! `cargo test` stays green in environments that don't have one.
//!
//! Everything here is offline -- no account is ever configured against a
//! mail server -- so it runs without network access.

use std::sync::Arc;
use std::time::Duration;

use deltachat_jsonrpc::{spawn_event_loop, RpcClient};
use serde_json::Value;

/// A 1x1 PNG, written out so the core reads real pixels rather than
/// guessing from a name: header, an IHDR saying 1x1 truecolour, one
/// compressed scanline, IEND.
const ONE_PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x03, 0x01, 0x01, 0x00, 0xc9, 0xfe, 0x92, 0xef, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A 1x1 GIF. The core classifies it as an animation and reports no
/// dimensions for it, which is the case the conversation has to survive.
const ONE_PIXEL_GIF: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xff, 0xff, 0xff,
    0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b,
];

/// One second of silence as a WAV: PCM, mono, 8 kHz, 8-bit, so the header
/// alone states the length. Built rather than checked in so the size the
/// assertions use is arithmetic rather than a magic number.
fn one_second_wav() -> Vec<u8> {
    let (rate, bits, channels) = (8000_u32, 8_u16, 1_u16);
    // One byte per sample at 8-bit mono, so a second is `rate` bytes.
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

/// A plain PNG `side` pixels square, stored rather than compressed.
///
/// Big enough that the core has to recode it: past
/// `BALANCED_IMAGE_BYTES` it scales the picture down and re-encodes it as
/// JPEG, and that is the difference the assertions below are about. Built
/// here for the reason `one_second_wav` is -- the sizes are then
/// arithmetic -- and stored uncompressed because a deflate
/// implementation is not worth carrying for a test fixture.
fn plain_png(side: usize) -> Vec<u8> {
    /// A length as a PNG writes one.
    fn span(value: usize) -> u32 {
        u32::try_from(value).unwrap_or(u32::MAX)
    }

    // A gradient rather than noise: what the core re-encodes has to come
    // out visibly smaller, and noise is what JPEG is worst at.
    let mut raw = Vec::with_capacity(side * (side * 3 + 1));
    for row in 0..side {
        raw.push(0); // filter: none
        for column in 0..side {
            let shade = u8::try_from((row + column) * 255 / (2 * side)).unwrap_or(255);
            raw.extend_from_slice(&[shade, shade / 2, 255 - shade]);
        }
    }

    // zlib: the usual header, then stored deflate blocks, then Adler-32.
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

    // A PNG chunk: its length, its name, its bytes, and the same CRC-32
    // a zip entry carries.
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
    // 8 bits a channel, truecolour, no interlacing.
    header.extend_from_slice(&[8, 2, 0, 0, 0]);

    let mut png = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    png.extend_from_slice(&chunk(b"IHDR", &header));
    png.extend_from_slice(&chunk(b"IDAT", &zlib));
    png.extend_from_slice(&chunk(b"IEND", &[]));
    png
}

/// Send one file to `chat` and hand back the message the core stored, which
/// is the shape every row is built from.
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

/// CRC-32 as zip stores it, so an archive built here is one a reader
/// will not reject.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            // The polynomial, applied only where the low bit was set.
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// A length as a zip field. The archive is a few hundred bytes written by
/// this file, so one that does not fit is a mistake here rather than a
/// case to carry a `Result` for.
fn field32(value: usize) -> u32 {
    match u32::try_from(value) {
        Ok(value) => value,
        Err(err) => panic!("the archive does not fit a zip: {err}"),
    }
}

/// The same, for the fields that are half as wide.
fn field16(value: usize) -> u16 {
    match u16::try_from(value) {
        Ok(value) => value,
        Err(err) => panic!("the archive does not fit a zip: {err}"),
    }
}

/// A .xdc: a zip of stored entries, which is all a webxdc is. Built here
/// rather than checked in, so what the core is asked about is readable.
///
/// Stored, not deflated: this repository has no compressor and wants
/// none -- the core reads the archive, and nothing in the app does.
fn webxdc(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut directory = Vec::new();
    for (name, body) in entries {
        let offset = field32(out.len());
        let crc = crc32(body);
        let size = field32(body.len());
        let name_len = field16(name.len());
        // Local file header, then the bytes themselves.
        out.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        for field in [20_u16, 0, 0, 0, 0x21] {
            // version needed, flags, method (stored), time, date
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(body);

        // And its entry in the central directory, which is what a reader
        // looks at first.
        directory.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        for field in [20_u16, 20, 0, 0, 0, 0x21] {
            // made by, version needed, flags, method, time, date
            directory.extend_from_slice(&field.to_le_bytes());
        }
        directory.extend_from_slice(&crc.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&name_len.to_le_bytes());
        for field in [0_u16, 0, 0, 0] {
            // extra, comment, disk, internal attributes
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
        // disk, first disk, entries here, entries in total
        out.extend_from_slice(&field.to_le_bytes());
    }
    out.extend_from_slice(&directory_size.to_le_bytes());
    out.extend_from_slice(&directory_at.to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

/// Standard base64 without padding, which is how the core hands a blob
/// over. Written out so a blob can be compared with the bytes it was made
/// from; the shim reads the same encoding (`webxdc_host.rs`).
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

/// Resolve the gate's value, treating a relative path as relative to the
/// repository root rather than to the process's working directory.
///
/// Cargo runs an integration test with its working directory set to the
/// *package* root, not the workspace or repository root, so the obvious
/// `DELTACHAT_RPC_SERVER=../vendor/...` -- which is what the README, the
/// Makefile and CI all naturally write -- would otherwise look for the
/// binary under `rust/`. That failure only shows up where the variable is
/// actually set, which is CI, and reads as a missing download rather than a
/// wrong path.
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
// One long function on purpose: this walks a single live session through
// the whole surface the app depends on, in order, against one spawned
// server. Splitting it into helpers would either respawn the core per step
// or hide the ordering that is half of what is being pinned.
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

    // Milestone 1 acceptance check: get_system_info answers over stdio.
    let info: std::collections::BTreeMap<String, String> = client
        .call_unit("get_system_info")
        .await
        .expect("get_system_info");
    assert!(
        info.contains_key("deltachat_core_version"),
        "unexpected get_system_info keys: {:?}",
        info.keys().collect::<Vec<_>>()
    );

    // Account bootstrap, exactly as DeltaChatCore::add_account does it.
    let account_id: u32 = client.call_unit("add_account").await.expect("add_account");

    // Config round trip, as configure_account does (without `configure`,
    // which would need a live mail server).
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

    // Start the event stream BEFORE doing something that emits an event.
    let (mut events, handle) = spawn_event_loop(client.clone());

    // Contact + chat + draft: all offline, and setting a draft emits a
    // MsgsChanged event (documented upstream), which proves real event
    // delivery end to end.
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

    // The chat list must contain the chat we just created, through the
    // same two calls DeltaChatCore::fetch_chat_list uses.
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

    // A chat holding a draft says so in its own summary, which is why
    // nothing in the app builds that text: `summaryText1` is the prefix a
    // row already shows in front of `summaryText2`, so a draft reads as
    // "Draft: ..." for free. `summaryStatus` 19 is DC_STATE_OUT_DRAFT, and
    // being under 20 is what keeps a delivery mark off a message that has
    // not been sent.
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

    // Reading one back. The core keeps drafts, so this is what survives
    // the app being closed -- and it comes back as a whole message object
    // rather than a string.
    let draft: Value = client
        .call("get_draft", (account_id, chat_id))
        .await
        .expect("get_draft");
    assert_eq!(
        draft.get("text").and_then(Value::as_str),
        Some("draft from integration test"),
        "the draft did not come back: {draft:?}"
    );

    // And clearing one. `misc_set_draft` with no text is not the way --
    // there is a call for it, and it answers null afterwards rather than
    // an empty message.
    client
        .call::<_, Value>("remove_draft", (account_id, chat_id))
        .await
        .expect("remove_draft");
    let gone: Value = client
        .call("get_draft", (account_id, chat_id))
        .await
        .expect("get_draft after remove");
    assert!(gone.is_null(), "a removed draft still reads back: {gone:?}");

    // And the message-list wire shape DeltaChatCore::fetch_messages relies
    // on: `kind` is "message" and the id field is snake_case `msg_id`
    // (this exact spelling was a bug once; keep it pinned by a test that
    // talks to the real core). A draft is not a message-list entry, so
    // just assert the call works and any entries present have the shape.
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

    // Account listing, as DeltaChatCore::refresh_accounts consumes it:
    // tagged `kind` with verbatim variant names ("Configured"/
    // "Unconfigured") and camelCase fields. Our never-configured test
    // account must show up as Unconfigured.
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

    // Which profile the app comes back to, as DeltaChatCore::refresh_accounts
    // and select_account speak it: the core selects an account it has just
    // added on its own, `select_account` takes the id bare, and
    // `get_selected_account_id` answers with the id or null -- an
    // `Option<u32>`, which is the shape the shim decodes.
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

    // QR classification, as DeltaChatCore::check_qr consumes it. A
    // DCACCOUNT: code is parsed purely locally (no network): expect
    // kind "account" (camelCase variant tag) with snake_case fields.
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

    // Encryption flags, as the chat list / message models consume them.
    // The email-address contact chat above is an unencrypted
    // "address-contact" chat; a freshly created group is encrypted.
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

    // A group after it is made, as ChatInfo reads it. The members are
    // ids, and the contacts behind them come from a second call keyed by
    // id -- as strings, JSON having no other kind of key.
    let full: Value = client
        .call("get_full_chat_by_id", (account_id, group_chat_id))
        .await
        .expect("get_full_chat_by_id");
    assert_eq!(
        full.get("chatType").and_then(Value::as_str),
        Some("Group"),
        "unexpected chat shape: {full:?}"
    );
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
    // The three edits, each its own call. An address contact cannot be
    // added to an encrypted group, which is why the shim reports a
    // refused member rather than failing the whole add.
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
    // Disappearing messages: seconds in, and read back on the chat as
    // `ephemeralTimer`. The core tells the other members itself.
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
    // Removing someone who is not in the group is not an error, and null
    // is how a picture is cleared -- of a group that never had one too.
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

    // Clearing the unread badge, as DeltaChatCore::open_chat does after
    // fetching messages. Pins the method name/params against the real
    // core (its MsgsNoticed side effect only fires when something was
    // actually fresh, so no event assertion here).
    client
        .call::<_, ()>("marknoticed_chat", (account_id, chat_id))
        .await
        .expect("marknoticed_chat");

    // Where the reader left off, which is where the conversation draws its
    // "new messages" line (`ChatMessages::load_unread_mark`). A chat with
    // nothing unseen in it answers with null rather than an error or a
    // zero, which is the whole of what the model reads.
    let nothing: Option<u32> = client
        .call("get_first_unread_message_of_chat", (account_id, chat_id))
        .await
        .expect("get_first_unread_message_of_chat on a read chat");
    assert_eq!(
        nothing, None,
        "a chat with nothing unread named a message to draw the line above"
    );

    // And with something unseen in it. A device message is the one
    // incoming message a core can be given offline: it lands in the
    // device chat as fresh, which is exactly the state the line is for.
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

    // The message object, as the conversation view consumes it. Saved
    // Messages is the one chat that sends offline, so it is where a real
    // message can be made to look at.
    // A second account, marked configured locally so the core will accept
    // a send. Nothing leaves the machine: Saved Messages has no recipient.
    let sender_id: u32 = client
        .call_unit("add_account")
        .await
        .expect("add_account for the message probe");
    for (key, value) in [
        ("configured_addr", "self@example.invalid"),
        ("displayname", "Testy"),
        ("configured", "1"),
    ] {
        client
            .call::<_, ()>("set_config", (sender_id, key, value))
            .await
            .expect("set_config");
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
                // Quoting the first message.
                Some(first),
            ),
        )
        .await
        .expect("misc_send_msg with a quote and a file");
    let messages: std::collections::HashMap<u32, Value> = client
        .call("get_messages", (sender_id, vec![first, second]))
        .await
        .expect("get_messages");
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

    // A long message is not sent whole: the core cuts the body and puts
    // the rest in an HTML part, and everything the app does about that
    // -- the notice while it is being written, the "view full message"
    // on the row, the page that shows it -- rests on this being true and
    // on `hasHtml` being how it is announced. The rule the notice uses
    // is 38 lines of up to 100 characters (`truncation.rs`), so fifty
    // lines is well past it.
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
    // And the other side of it: a short message is whole where it
    // stands, and asking for an HTML part it has not got must not be an
    // error -- the reader page asks only when `hasHtml` says to, and
    // this is what that flag is worth.
    let short_message: Value = client
        .call("get_message", (sender_id, first))
        .await
        .expect("get_message for a short one");
    assert_eq!(
        short_message.get("hasHtml").and_then(Value::as_bool),
        Some(false),
        "a three-word message was marked as one the core had to cut:          {short_message:?}"
    );

    // The core decides the view type from the file, and the conversation
    // renders a picture inline on the strength of that: nothing in the app
    // classifies an attachment, and nothing should start.
    let picture = std::env::temp_dir().join("piirit-real-server-dot.png");
    // The smallest valid PNG: an 1x1 image, so the core has real pixels to
    // read rather than a name to guess from.
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

    // The core leaves a picture sent as a `File` at the size it was given
    // and recodes one named `Image`. That is the whole reason the app
    // names a picture before sending it (piirit-shim/src/media.rs):
    // every message it composes would otherwise go out as a `File`, and
    // the outgoing media quality setting, which the core reads only while
    // recoding, would do nothing at all. A picture past
    // `BALANCED_IMAGE_BYTES` is what makes the difference visible.
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

    // 0 is the core's own default, and what the settings page calls
    // balanced; 1 is what it calls worse quality.
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
    // The other half of naming a picture, and the reason the bar's name
    // and the recipient's differ for one: the core replaces an image's
    // filename with a dated one of its own, deliberately, because a
    // camera's own name carries a timestamp and a running number. It is
    // a JPEG by then whatever went in.
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

    // What the core recommends as the largest attachment for this
    // profile's relay, which is what the conversation refuses a bigger
    // file on the strength of. A computed `sys.` key rather than anything
    // stored, so it answers on an account that has never been asked.
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

    // What the core does NOT fill in, which is as much of the contract as
    // what it does: a GIF gets no dimensions and a sound file gets no
    // duration, so the conversation must not size or label anything from
    // them without checking. This is the whole reason AttachmentPreview
    // falls back to the loaded item's own proportions and lets the audio
    // player report its own length.
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

    // A voice message, as ChatMessages::send_voice sends one: `send_msg`
    // with a MessageData naming the view type, since the core would
    // classify the same file as Audio on its own (below). Answers with the
    // id alone, and the row read back carries the type asked for.
    let voice_id: u32 = client
        .call(
            "send_msg",
            (
                sender_id,
                saved,
                serde_json::json!({
                    "file": tone.to_string_lossy(),
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

    // The chat's own index of what it holds, which the media pages are
    // built on (piirit-shim/src/chat_media.rs): up to three view types
    // in one call, the chat optional, and the ids come back oldest first
    // -- the voice message went before the tone, so it stands before it.
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

    // A shared contact. The core parses the card and hands back the pieces
    // a contact row is built from, so nothing here reads vCard syntax.
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

    // A webxdc app, and the four calls running one is made of. Nothing in
    // this repository opens the archive: the shim serves an app out of
    // `get_webxdc_blob` (piirit-shim/src/webxdc_host.rs), so a wrong
    // assumption about any of these is a blank page on a phone and
    // nothing anywhere else.
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
    // Sent as a plain file, the way the attach tray sends one: the core
    // is what turns a .xdc into an app.
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

    // What the row draws, and what the app is told about itself.
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

    // The files, as the host serves them: base64 without padding, which
    // is what the shim's decoder is written for.
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

    // Updates both ways: the POST the bridge sends, and the poll it
    // collects with.
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
    // A JSON *string*, not an array: the host hands it to the app
    // unread, so its shape is the app's contract rather than ours.
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

    // Chat kind, which decides whether a message names its sender.
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

    // The chat-list row, as the list consumes it. Every field a row shows
    // has to be there and spelled the way the core spells it.
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
    // The chat with oneself says so, which is how the cover keeps it out
    // of the people it draws; the group is nobody's self.
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

    // Who `get_contacts` lists. With listFlags 0 the account's own
    // contact (DC_CONTACT_ID_SELF, 1) is left out; DC_GCL_ADD_SELF (2)
    // appends it, and a query drops it again. The new-group page draws
    // the reader among the members from the first, and the pickers from
    // the second.
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

    // Blocking, which the core keeps a list of its own for: a blocked
    // contact is listed by `get_blocked_contacts` -- which takes the
    // account and nothing else -- as a whole contact object, the same
    // shape a row is drawn from, and each way round is announced as a
    // `ContactsChanged`, which is all an open page is told.
    //
    // That a blocked contact leaves `get_contacts` is not asserted here
    // and cannot be: everything this offline session can make is an
    // address contact, which the core's contact listing leaves out
    // whether it is blocked or not (`get_contacts` above answers with
    // the account's own contact and nothing else). The fake core models
    // the exclusion, and `piirit-shim/tests/blocking.rs` drives this
    // side of it.
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

    // What the row's context menu does. Visibility is one method with the
    // core's own variant names, and muting takes a tagged duration.
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

    // What a message's context menu does. Both take a list, not a single
    // id, and a reply is `misc_send_msg` with the quoted message last --
    // pinned by the quote assertion above.
    client
        .call::<_, ()>("resend_messages", (sender_id, vec![first]))
        .await
        .expect("resend_messages");
    client
        .call::<_, ()>("delete_messages", (sender_id, vec![second]))
        .await
        .expect("delete_messages");

    // Read receipts, as ChatMessages sends them when a chat is opened.
    client
        .call::<_, ()>("markseen_msgs", (sender_id, vec![first]))
        .await
        .expect("markseen_msgs");

    // Reactions, as ChatMessages sends and draws them. The call takes the
    // whole list of this account's reactions on the message and answers
    // with the id of the hidden message carrying it; the message object
    // then carries them counted and sorted, with a flag for our own, and
    // reads null once an empty list has taken them off again. A reaction
    // is a message the list does not show, so the chat's id list must not
    // grow by it.
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
    // The event the model refreshes the row on, with the message named.
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
        cleared[&first]
            .get("reactions")
            .is_some_and(serde_json::Value::is_null),
        "an empty list did not take the reaction off: {:?}",
        cleared[&first]
    );

    // What the next round of UI work depends on, pinned here because a
    // wrong method name or config key fails only on a device -- the fake
    // core answers to whatever it is asked.

    // Forwarding. `isForwarded` is what marks a forwarded copy for the
    // sender as well as the recipient, and it is false on the original:
    // an assertion on the copy alone would pass against a field that was
    // simply always true.
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

    // Searching. Three arguments, the last being an optional chat to
    // search within -- passing two is rejected outright.
    let _: Vec<u32> = client
        .call(
            "search_messages",
            (sender_id, "forward", Option::<u32>::None),
        )
        .await
        .expect("search_messages takes (account, query, chat_id option)");

    // The profile fields a settings page edits. `displayname` is already
    // pinned above; these are the two it does not cover.
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

    // `selfavatar` is a *path to an image the core copies into its blob
    // directory*, not a value: an empty string is rejected outright with
    // "Copying new blobfile failed". A settings page has to hand it a
    // real file, and clear it with null rather than "".
    let avatar = std::env::temp_dir().join("piirit-real-server-avatar.png");
    // The smallest valid PNG; the core rejects what it cannot decode.
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
    // Null clears it. This is the shape a "remove picture" action needs.
    client
        .call::<_, ()>(
            "set_config",
            (sender_id, "selfavatar", Option::<String>::None),
        )
        .await
        .expect("set_config selfavatar null clears it");
    let _ = std::fs::remove_file(&avatar);

    // The account list, as the profiles page draws it: a configured
    // account carries its picture and its colour.
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
    for field in ["displayName", "addr", "profileImage", "color"] {
        assert!(
            configured.get(field).is_some(),
            "a configured account lost the {field} field, which the profiles \
             page draws: {configured:?}"
        );
    }

    // A name of the reader's own for a contact, and the way back to the
    // contact's: an empty name. The contact page is built on the empty
    // name meaning that rather than being refused.
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

    // What the profile page says about the connection and the mailbox:
    // a band, a report written for a web view, and the profile's size on
    // the device. Offline, so the band says so and the report holds no
    // quota -- what is pinned is that the calls exist and answer in the
    // shapes connectivity.rs reads.
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

    // The download limit, as the settings page writes it to every
    // account: a config key, in bytes, read back as written.
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
    // Marking a chat unread, and counting what is unread: the calls the
    // chat list's menu and the badges on the profiles page are built on.
    // The saved-messages chat holds only outgoing messages, so there is
    // nothing offline for markfresh_chat to put back to fresh; what is
    // pinned is that both calls exist and answer in the shapes
    // chatlist.rs and core.rs read.
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

    // And the field a held-back message is told apart by, on one that
    // is not: the delegate offers the rest on anything but Done.
    assert_eq!(
        reply.get("downloadState").and_then(Value::as_str),
        Some("Done"),
        "a message sent from here is not marked as fully there: {reply:?}"
    );

    let _ = std::fs::remove_file(&attachment);
    handle.stop();
    client.shutdown().await.expect("shutdown");
    let _ = std::fs::remove_dir_all(&accounts_dir);
}
