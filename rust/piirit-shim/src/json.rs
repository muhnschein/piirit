//! Field readers for core JSON: no typed structs (`docs/PROJECT.md`).
//!
//! Absent or null means empty, zero or false; decided here, once.
//!
//! `path`: bare name, or JSON pointer (`/quote/text`).

use qmetaobject::QString;
use serde_json::Value;

/// Field at `path`.
fn field<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.starts_with('/') {
        value.pointer(path)
    } else {
        value.get(path)
    }
}

/// String field as `&str`; `""` if absent.
pub(crate) fn str_at<'a>(value: &'a Value, path: &str) -> &'a str {
    field(value, path)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// String field for a model row; empty if absent.
pub(crate) fn text(value: &Value, path: &str) -> QString {
    str_at(value, path).into()
}

/// Id field, or `None`. Above `u32` counts as absent.
pub(crate) fn u32_opt(value: &Value, path: &str) -> Option<u32> {
    field(value, path)
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
}

/// Counter or state; 0 is valid.
pub(crate) fn u32_at(value: &Value, path: &str) -> u32 {
    u32_opt(value, path).unwrap_or(0)
}

/// Whole number, 0 if absent.
pub(crate) fn i32_at(value: &Value, path: &str) -> i32 {
    field(value, path)
        .and_then(Value::as_i64)
        .and_then(|number| i32::try_from(number).ok())
        .unwrap_or(0)
}

/// Timestamp, 0 if absent.
pub(crate) fn i64_at(value: &Value, path: &str) -> i64 {
    field(value, path).and_then(Value::as_i64).unwrap_or(0)
}

/// Event payload if about this chat, else `None`.
///
/// `MsgsChanged` has `chatId` 0 (several chats); overflow has none. Both are every chat's.
pub(crate) fn chat_event(payload_json: &str, chat_id: u32) -> Option<Value> {
    let payload: Value = serde_json::from_str(payload_json).unwrap_or_default();
    let event_chat = u32_at(&payload, "chatId");
    (event_chat == 0 || event_chat == chat_id).then_some(payload)
}

/// Flag, false if absent.
pub(crate) fn flag(value: &Value, path: &str) -> bool {
    field(value, path).and_then(Value::as_bool).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{chat_event, flag, i32_at, i64_at, str_at, text, u32_at, u32_opt};
    use serde_json::json;

    #[test]
    fn a_bare_name_and_a_pointer_read_the_same_object() {
        let message = json!({
            "text": "hello",
            "chatId": 7,
            "timestamp": -1,
            "isInfo": true,
            "dimensionsWidth": 640,
            "quote": { "text": "earlier" },
        });
        assert_eq!(str_at(&message, "text"), "hello");
        assert_eq!(text(&message, "/quote/text").to_string(), "earlier");
        assert_eq!(u32_opt(&message, "chatId"), Some(7));
        assert_eq!(u32_at(&message, "chatId"), 7);
        assert_eq!(i64_at(&message, "timestamp"), -1);
        assert_eq!(i32_at(&message, "dimensionsWidth"), 640);
        assert!(flag(&message, "isInfo"));
    }

    #[test]
    fn absent_null_and_wrongly_typed_fields_read_as_nothing() {
        let message = json!({ "text": null, "chatId": "7", "big": u64::MAX, "neg": -5 });
        assert_eq!(str_at(&message, "text"), "");
        assert_eq!(str_at(&message, "missing"), "");
        assert_eq!(text(&message, "/quote/text").to_string(), "");
        // Not a number.
        assert_eq!(u32_opt(&message, "chatId"), None);
        assert_eq!(u32_at(&message, "chatId"), 0);
        // Out of range: absent.
        assert_eq!(u32_opt(&message, "big"), None);
        assert_eq!(u32_opt(&message, "neg"), None);
        assert_eq!(i32_at(&message, "neg"), -5);
        assert_eq!(i64_at(&message, "missing"), 0);
        assert!(!flag(&message, "missing"));
    }

    #[test]
    fn an_event_is_the_chats_own_when_it_names_it_or_names_none() {
        assert!(chat_event(r#"{"chatId":7,"msgId":3}"#, 7).is_some());
        assert!(chat_event(r#"{"chatId":8,"msgId":3}"#, 7).is_none());
        // Several chats, or no chat.
        assert!(chat_event(r#"{"chatId":0}"#, 7).is_some());
        assert!(chat_event("{}", 7).is_some());
        // Not JSON: empty object.
        let payload = chat_event("nonsense", 7);
        assert_eq!(
            payload.as_ref().map(|value| u32_at(value, "msgId")),
            Some(0)
        );
    }
}
