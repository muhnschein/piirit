//! Qt/QML integration layer over `deltachat-jsonrpc`: [`DeltaChatCore`] as a
//! `QObject`, chat and message lists as `QAbstractListModel`s.
//!
//! Needs Qt5 dev headers to build.

// All four are forced by qmetaobject's derive macros, not by code here:
// the macros expand to a wide, unstable set of items from that crate
// (`wildcard_imports`), generate the `QObject` impl and dispatcher
// (`useless_transmute`, `type_complexity`), and require by-value `QString`
// parameters (`needless_pass_by_value`).
#![allow(
    clippy::wildcard_imports,
    clippy::useless_transmute,
    clippy::type_complexity,
    clippy::needless_pass_by_value
)]

mod backup;
mod capture;
mod chat;
mod chat_info;
mod chat_media;
mod chatlist;
mod connectivity;
mod contacts;
mod core;
mod full_text;
mod html;
mod json;
mod links;
mod markdown;
mod media;
mod models;
mod prefetch;
mod profile;
mod qr;
mod recorder;
mod runtime;
mod saver;
mod search;
mod signup;
mod truncation;
mod webxdc;
mod webxdc_host;

pub use crate::backup::Backup;
pub use crate::capture::Captures;
pub use crate::chat::{local_day_number, ChatMessages};
pub use crate::chat_info::ChatInfo;
pub use crate::chat_media::ChatMedia;
pub use crate::chatlist::ChatList;
pub use crate::contacts::ContactList;
pub use crate::core::{
    server_path, server_pid, shutdown, DeltaChatCore, BUNDLED_SERVER, HANDLED_EVENT_KINDS,
};
pub use crate::full_text::FullText;
pub use crate::prefetch::ChatPrefetch;
pub use crate::profile::Profile;
pub use crate::qr::{QrCode, QrScanner};
pub use crate::recorder::VoiceRecorder;
pub use crate::saver::FileSaver;
pub use crate::search::SearchResults;
pub use crate::webxdc::{WebxdcApp, WebxdcStore};

/// Register the shim's QML-instantiable types. The app and the tests share
/// this so a page cannot work in one and not the other.
pub fn register_qml_types() {
    // Nul-terminated literals; `c"..."` would be neater but arrived in Rust
    // 1.77, past the 1.75 floor Sailfish sets.
    let (Ok(uri), Ok(messages), Ok(list)) = (
        std::ffi::CStr::from_bytes_with_nul(b"Postivene\0"),
        std::ffi::CStr::from_bytes_with_nul(b"ChatMessages\0"),
        std::ffi::CStr::from_bytes_with_nul(b"ChatList\0"),
    ) else {
        return;
    };
    let (Ok(contacts), Ok(search), Ok(profile), Ok(prefetch)) = (
        std::ffi::CStr::from_bytes_with_nul(b"ContactList\0"),
        std::ffi::CStr::from_bytes_with_nul(b"SearchResults\0"),
        std::ffi::CStr::from_bytes_with_nul(b"Profile\0"),
        std::ffi::CStr::from_bytes_with_nul(b"ChatPrefetch\0"),
    ) else {
        return;
    };
    let (Ok(info), Ok(media), Ok(full)) = (
        std::ffi::CStr::from_bytes_with_nul(b"ChatInfo\0"),
        std::ffi::CStr::from_bytes_with_nul(b"ChatMedia\0"),
        std::ffi::CStr::from_bytes_with_nul(b"FullText\0"),
    ) else {
        return;
    };
    let (Ok(qr_code), Ok(qr_scanner), Ok(saver)) = (
        std::ffi::CStr::from_bytes_with_nul(b"QrCode\0"),
        std::ffi::CStr::from_bytes_with_nul(b"QrScanner\0"),
        std::ffi::CStr::from_bytes_with_nul(b"FileSaver\0"),
    ) else {
        return;
    };
    let (Ok(captures), Ok(recorder), Ok(webxdc), Ok(store)) = (
        std::ffi::CStr::from_bytes_with_nul(b"Captures\0"),
        std::ffi::CStr::from_bytes_with_nul(b"VoiceRecorder\0"),
        std::ffi::CStr::from_bytes_with_nul(b"WebxdcApp\0"),
        std::ffi::CStr::from_bytes_with_nul(b"WebxdcStore\0"),
    ) else {
        return;
    };
    let Ok(backup) = std::ffi::CStr::from_bytes_with_nul(b"Backup\0") else {
        return;
    };
    qmetaobject::qml_register_type::<ChatMessages>(uri, 1, 0, messages);
    qmetaobject::qml_register_type::<ChatList>(uri, 1, 0, list);
    qmetaobject::qml_register_type::<ContactList>(uri, 1, 0, contacts);
    qmetaobject::qml_register_type::<SearchResults>(uri, 1, 0, search);
    qmetaobject::qml_register_type::<Profile>(uri, 1, 0, profile);
    qmetaobject::qml_register_type::<ChatPrefetch>(uri, 1, 0, prefetch);
    qmetaobject::qml_register_type::<ChatInfo>(uri, 1, 0, info);
    qmetaobject::qml_register_type::<ChatMedia>(uri, 1, 0, media);
    qmetaobject::qml_register_type::<FullText>(uri, 1, 0, full);
    qmetaobject::qml_register_type::<QrCode>(uri, 1, 0, qr_code);
    qmetaobject::qml_register_type::<QrScanner>(uri, 1, 0, qr_scanner);
    qmetaobject::qml_register_type::<FileSaver>(uri, 1, 0, saver);
    qmetaobject::qml_register_type::<Captures>(uri, 1, 0, captures);
    qmetaobject::qml_register_type::<VoiceRecorder>(uri, 1, 0, recorder);
    qmetaobject::qml_register_type::<WebxdcApp>(uri, 1, 0, webxdc);
    qmetaobject::qml_register_type::<WebxdcStore>(uri, 1, 0, store);
    qmetaobject::qml_register_type::<Backup>(uri, 1, 0, backup);
}
pub use models::{
    AccountItem, ChatListItem, ChatListModel, ContactItem, ContactListModel, MessageListItem,
    MessageListModel, SearchItem, SearchListModel,
};
