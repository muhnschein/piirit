//! Qt/QML layer over `deltachat-jsonrpc`: [`DeltaChatCore`] is a `QObject`,
//! chat and message lists are `QAbstractListModel`s.
//!
//! Needs Qt5 dev headers to build.

// Phone code: panic silently ends Qt callback or tokio task; release overflow wraps. Not
// Cargo.toml lints: can't exempt tests (other crates or `cfg(test)`, as build scripts, src/bin).
#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::string_slice,
        clippy::arithmetic_side_effects,
        clippy::let_underscore_must_use,
        clippy::panic,
        clippy::panic_in_result_fn,
        clippy::unwrap_in_result,
        clippy::get_unwrap
    )
)]
// Forced by qmetaobject macros: wildcard imports, generated QObject impl, by-value QString.
#![allow(
    clippy::wildcard_imports,
    clippy::useless_transmute,
    clippy::type_complexity,
    clippy::needless_pass_by_value
)]
// cpp! macro recursion: default 128 too low for pictures.rs; mic_watch.rs needs >1024.
#![recursion_limit = "2048"]

mod backup;
mod call_host;
mod calls;
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
mod location;
mod markdown;
mod media;
mod mic_watch;
mod models;
mod pictures;
mod prefetch;
mod profile;
mod qr;
mod recorder;
mod runtime;
mod saver;
mod search;
mod second_device;
mod signup;
mod transports;
mod video;
mod voice;
mod webxdc;
mod webxdc_host;

pub use crate::backup::Backup;
pub use crate::call_host::prepare_media_plugins;
pub use crate::calls::Call;
pub use crate::capture::Captures;
pub use crate::chat::{local_day_number, ChatMessages};
pub use crate::chat_info::ChatInfo;
pub use crate::chat_media::ChatMedia;
pub use crate::chatlist::ChatList;
pub use crate::contacts::ContactList;
pub use crate::core::{
    server_path, server_pid, shutdown, DeltaChatCore, BUNDLED_SERVER, DEFAULT_PROVIDER_QR,
    HANDLED_EVENT_KINDS,
};
pub use crate::full_text::FullText;
pub use crate::location::{LiveLocation, SharedLocation};
pub use crate::mic_watch::{MicState, MicWatch, MicWatcher};
pub use crate::pictures::{install as install_pictures, render as render_picture};
pub use crate::prefetch::ChatPrefetch;
pub use crate::profile::Profile;
pub use crate::qr::{QrCode, QrScanner};
pub use crate::recorder::VoiceRecorder;
pub use crate::saver::FileSaver;
pub use crate::search::SearchResults;
pub use crate::second_device::SecondDevice;
pub use crate::transports::Transports;
pub use crate::webxdc::{WebxdcApp, WebxdcStore};

/// Register shim's QML types; app and tests.
pub fn register_qml_types() {
    // `c"..."` literals need Rust 1.77; Sailfish floor is 1.75.
    let (Ok(uri), Ok(messages), Ok(list)) = (
        std::ffi::CStr::from_bytes_with_nul(b"Piirit\0"),
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
    let (Ok(backup), Ok(transports), Ok(second_device), Ok(call)) = (
        std::ffi::CStr::from_bytes_with_nul(b"Backup\0"),
        std::ffi::CStr::from_bytes_with_nul(b"Transports\0"),
        std::ffi::CStr::from_bytes_with_nul(b"SecondDevice\0"),
        std::ffi::CStr::from_bytes_with_nul(b"Call\0"),
    ) else {
        return;
    };
    let (Ok(mic_watch), Ok(shared_location), Ok(live_location)) = (
        std::ffi::CStr::from_bytes_with_nul(b"MicWatch\0"),
        std::ffi::CStr::from_bytes_with_nul(b"SharedLocation\0"),
        std::ffi::CStr::from_bytes_with_nul(b"LiveLocation\0"),
    ) else {
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
    qmetaobject::qml_register_type::<Transports>(uri, 1, 0, transports);
    qmetaobject::qml_register_type::<SecondDevice>(uri, 1, 0, second_device);
    qmetaobject::qml_register_type::<Call>(uri, 1, 0, call);
    qmetaobject::qml_register_type::<MicWatch>(uri, 1, 0, mic_watch);
    qmetaobject::qml_register_type::<SharedLocation>(uri, 1, 0, shared_location);
    qmetaobject::qml_register_type::<LiveLocation>(uri, 1, 0, live_location);
}
pub use models::{
    AccountItem, ChatListItem, ChatListModel, ContactItem, ContactListModel, MessageListItem,
    MessageListModel, SearchItem, SearchListModel, TransportItem, TransportListModel,
};
