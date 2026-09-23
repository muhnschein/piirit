//! What a picture or a video costs to send, and what the relay will take.
//!
//! Two questions the conversation asks about a file before it hands it
//! over, and both are the core's own answers rather than rules of ours.
//!
//! The first is what to call the file. A message composed here goes
//! through `misc_send_msg`, which names its attachment `File`, and the
//! core leaves a `File` at its original size on purpose -- that is how
//! every client offers "send this picture uncompressed". A picture the
//! reader picked out of the gallery or took a moment ago is not that, so
//! it is named `Image` instead, which is what puts it through the core's
//! own recoding and so through the outgoing media quality setting. The
//! suffixes here are the ones the core itself reads as `Image`
//! (`guess_msgtype_from_path_suffix`); anything else it either
//! recognises on its own -- a video, a sound, an app -- or sends as it
//! is.
//!
//! Naming a picture costs its name: the core sends one as
//! `image_<date>.jpg` rather than as whatever it was called, on purpose,
//! since a camera's own filename is a timestamp and a running number.
//! That and the recoding are what every other client's pictures already
//! do, and both are pinned against the real core by
//! `deltachat-jsonrpc/tests/real_server.rs`.
//!
//! The second is whether the file will go at all. The core answers that
//! with `sys.msgsize_max_recommended`, the largest attachment it
//! recommends, and says of it that a UI may refuse a bigger one. It is
//! the core's own constant (`RECOMMENDED_FILE_SIZE`, about 22 MB), the
//! same whichever relay mail leaves through, so nothing that shows
//! it should call it the relay's or expect it to change with the relay.
//! Nothing here refuses a picture: the core is about to shrink it, and
//! the size on the phone says nothing about the size that leaves.

use deltachat_jsonrpc::RpcClient;

/// The config key the core answers its attachment ceiling with, in
/// bytes.
const LIMIT_KEY: &str = "sys.msgsize_max_recommended";

/// The suffixes the core reads as a picture, and so the files it recodes
/// on the way out. Deliberately not every image format: `heic`, `tiff`
/// and `avif` are files to the core, and a `gif` is an animation it
/// would spoil by recoding.
const PICTURES: [&str; 5] = ["jpg", "jpeg", "jpe", "png", "webp"];

/// What the core should call this attachment, or `None` to let it decide
/// from the file itself.
pub(crate) fn outgoing_viewtype(path: &str) -> Option<&'static str> {
    is_picture(path).then_some("Image")
}

/// Whether the core will recode this file on its way out.
pub(crate) fn is_picture(path: &str) -> bool {
    let suffix = std::path::Path::new(path)
        .extension()
        .map(|suffix| suffix.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    PICTURES.contains(&suffix.as_str())
}

/// What this file weighs on the phone, 0 when it cannot be measured.
pub(crate) fn file_bytes(path: &str) -> u64 {
    if path.is_empty() {
        return 0;
    }
    std::fs::metadata(path).map(|file| file.len()).unwrap_or(0)
}

/// Whether this file is bigger than the relay takes.
///
/// False for a picture, whatever it weighs: the core recodes those, and
/// what it sends is not what is on the phone. False as well when the
/// limit is not known yet, or the file cannot be measured -- a question
/// that cannot be answered is not an answer of "too big".
pub(crate) fn exceeds_limit(path: &str, limit: u64) -> bool {
    if limit == 0 || is_picture(path) {
        return false;
    }
    file_bytes(path) > limit
}

/// The largest attachment the core recommends, in bytes; 0 when the
/// core would not say. The same for every relay; see the module doc.
pub(crate) async fn attachment_limit(rpc: &RpcClient, account_id: u32) -> u64 {
    rpc.call::<_, Option<String>>("get_config", (account_id, LIMIT_KEY))
        .await
        .ok()
        .flatten()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{exceeds_limit, file_bytes, is_picture, outgoing_viewtype};

    #[test]
    fn the_pictures_the_core_recodes_are_the_ones_it_is_told_about() {
        for path in [
            "/home/user/Pictures/holiday.jpg",
            "/home/user/Pictures/HOLIDAY.JPG",
            "photo-20260904-151212.jpeg",
            "scan.jpe",
            "diagram.png",
            "sticker.webp",
        ] {
            assert!(is_picture(path), "{path}");
            assert_eq!(outgoing_viewtype(path), Some("Image"), "{path}");
        }
    }

    #[test]
    fn everything_else_is_left_for_the_core_to_name() {
        for path in [
            // A video, a sound and an app: the core reads all three off
            // the suffix itself.
            "video-20260904-151212.mp4",
            "voice-20260904-151212.ogg",
            "game.xdc",
            // Images the core does not recode, animation and all.
            "party.gif",
            "photo.heic",
            "scan.tiff",
            // No suffix at all.
            "/home/user/Documents/notes",
            "",
        ] {
            assert!(!is_picture(path), "{path}");
            assert_eq!(outgoing_viewtype(path), None, "{path}");
        }
    }

    #[test]
    fn a_file_over_the_relays_ceiling_is_too_big() {
        let dir = std::env::temp_dir().join(format!("piirit-media-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let video = dir.join("video.mp4");
        std::fs::write(&video, vec![0_u8; 2048]).expect("write the video");
        let path = video.to_string_lossy().into_owned();

        assert_eq!(file_bytes(&path), 2048);
        assert!(exceeds_limit(&path, 1024));
        assert!(!exceeds_limit(&path, 2048), "the limit itself still fits");
        assert!(!exceeds_limit(&path, 4096));
        // Not known yet, so nothing is refused on the strength of it.
        assert!(!exceeds_limit(&path, 0));

        // A picture of the same size is the core's to shrink.
        let picture = dir.join("photo.jpg");
        std::fs::write(&picture, vec![0_u8; 2048]).expect("write the picture");
        assert!(!exceeds_limit(&picture.to_string_lossy(), 1024));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_not_there_is_not_too_big() {
        assert_eq!(file_bytes("/nowhere/at/all/video.mp4"), 0);
        assert_eq!(file_bytes(""), 0);
        assert!(!exceeds_limit("/nowhere/at/all/video.mp4", 1024));
        assert!(!exceeds_limit("", 1024));
    }
}
