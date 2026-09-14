//! Reading fields off the core's JSON objects.
//!
//! The core's answers are `serde_json::Value`s read field by field rather
//! than deserialised into structs: typing its several dozen shapes would be
//! the protocol reimplementation `docs/PROJECT.md` rules out, and a field
//! that is absent or null is the ordinary case, not an error. So "absent
//! means empty, zero or false" is decided here, once, rather than
//! separately in every model.
//!
//! `path` is either a bare field name (`"chatId"`) or a JSON pointer
//! (`"/quote/text"`) for a field inside a nested object.

use qmetaobject::QString;
use serde_json::Value;

/// The field itself, whichever way it was named.
fn field<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.starts_with('/') {
        value.pointer(path)
    } else {
        value.get(path)
    }
}

/// A string field as a `&str`, `""` when absent or not a string. For
/// callers that want to look at the text before storing it.
pub(crate) fn str_at<'a>(value: &'a Value, path: &str) -> &'a str {
    field(value, path)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// A string field for a model row, empty when absent or not a string.
pub(crate) fn text(value: &Value, path: &str) -> QString {
    str_at(value, path).into()
}

/// An unsigned field that has to be there -- an id -- or `None`. Out of
/// `u32` range counts as absent: nothing the core numbers goes that high.
pub(crate) fn u32_opt(value: &Value, path: &str) -> Option<u32> {
    field(value, path)
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
}

/// An unsigned field where 0 is a fine answer: a counter, a state.
pub(crate) fn u32_at(value: &Value, path: &str) -> u32 {
    u32_opt(value, path).unwrap_or(0)
}

/// A whole number that may be absent often enough for 0 to be the normal
/// case: pixel dimensions, durations.
pub(crate) fn i32_at(value: &Value, path: &str) -> i32 {
    field(value, path)
        .and_then(Value::as_i64)
        .and_then(|number| i32::try_from(number).ok())
        .unwrap_or(0)
}

/// A timestamp, 0 when absent.
pub(crate) fn i64_at(value: &Value, path: &str) -> i64 {
    field(value, path).and_then(Value::as_i64).unwrap_or(0)
}

/// The payload of a core event, when the event is about one chat: an
/// event names its chat in `chatId`, except that `MsgsChanged` carries 0
/// for "several chats" and an overflow carries none at all, and both of
/// those are every chat's. `None` for another chat's event, so a model
/// that holds one chat can drop it in a line. Every model that listens to
/// a chat asks this rather than each reading the field for itself.
pub(crate) fn chat_event(payload_json: &str, chat_id: u32) -> Option<Value> {
    let payload: Value = serde_json::from_str(payload_json).unwrap_or_default();
    let event_chat = u32_at(&payload, "chatId");
    (event_chat == 0 || event_chat == chat_id).then_some(payload)
}

/// A flag, false when absent.
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
        // A string where a number belongs is not a number.
        assert_eq!(u32_opt(&message, "chatId"), None);
        assert_eq!(u32_at(&message, "chatId"), 0);
        // Out of range is absent, not truncated.
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
        // MsgsChanged for several chats, and an overflow with no chat.
        assert!(chat_event(r#"{"chatId":0}"#, 7).is_some());
        assert!(chat_event("{}", 7).is_some());
        // Not JSON at all reads as an empty object: every chat's, and
        // every field of it absent.
        let payload = chat_event("nonsense", 7);
        assert_eq!(
            payload.as_ref().map(|value| u32_at(value, "msgId")),
            Some(0)
        );
    }
}
