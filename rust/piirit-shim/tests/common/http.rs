//! Speaking to a loopback host the way a page does: the call host's
//! tests play the page's side of the conversation over a real socket.

use std::io::{Read, Write};
use std::time::Duration;

/// One request, its whole answer. `Err` is nothing listening.
pub fn ask(authority: &str, head: &str, body: &[u8]) -> Result<String, String> {
    let mut stream =
        std::net::TcpStream::connect(authority).map_err(|err| format!("connect: {err}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|err| err.to_string())?;
    stream
        .write_all(head.as_bytes())
        .map_err(|err| format!("write: {err}"))?;
    stream
        .write_all(body)
        .map_err(|err| format!("write-body: {err}"))?;
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .map_err(|err| format!("read: {err}"))?;
    Ok(String::from_utf8_lossy(&answer).into_owned())
}

pub fn get_as(authority: &str, host: &str, path: &str) -> String {
    ask(
        authority,
        &format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"),
        &[],
    )
    .unwrap_or_else(|err| err)
}

pub fn get(authority: &str, path: &str) -> String {
    get_as(authority, authority, path)
}

/// A POST the way the bridge sends one: the text as it is.
pub fn post(authority: &str, path: &str, body: &str) -> String {
    ask(
        authority,
        &format!(
            "POST {path} HTTP/1.1\r\nHost: {authority}\r\n\
             Content-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n",
            body.len()
        ),
        body.as_bytes(),
    )
    .unwrap_or_else(|err| err)
}

pub fn status_of(answer: &str) -> &str {
    answer.lines().next().unwrap_or_default()
}

pub fn body_of(answer: &str) -> &str {
    answer.split_once("\r\n\r\n").map_or("", |(_, body)| body)
}

pub fn authority_of(url: &str) -> String {
    let rest = url.strip_prefix("http://").unwrap_or(url);
    rest.split('/').next().unwrap_or_default().to_string()
}

/// The API path a page was given: the key in its own address, which is
/// the only place the key is.
pub fn api_of(url: &str) -> String {
    let key = url
        .split(['?', '&', '#'])
        .find_map(|part| part.strip_prefix("key="))
        .unwrap_or_default();
    format!("/call-api/{key}")
}

/// A hash command's SDP, decoded the way the page decodes it:
/// `decodeURIComponent`, then `atob`.
pub fn payload_of(command: &str, name: &str) -> String {
    let Some(encoded) = command.strip_prefix(&format!("{name}=")) else {
        return format!("not a {name}: {command}");
    };
    let mut bytes = Vec::new();
    let raw = encoded.as_bytes();
    let mut at = 0;
    while at < raw.len() {
        if raw[at] == b'%' && at + 2 < raw.len() {
            let hex = std::str::from_utf8(&raw[at + 1..at + 3]).unwrap_or("00");
            bytes.push(u8::from_str_radix(hex, 16).unwrap_or(0));
            at += 3;
        } else {
            bytes.push(raw[at]);
            at += 1;
        }
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut buffer, mut bits) = (0_u32, 0_u32);
    for c in text.chars() {
        if c == '=' {
            break;
        }
        let Some(value) = alphabet.find(c) else {
            return format!("not base64: {text}");
        };
        buffer = (buffer << 6) | u32::try_from(value).unwrap_or(0);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xff).unwrap_or(0));
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
