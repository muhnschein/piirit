//! Attachment size and picture naming for outgoing files.
//!
//! `misc_send_msg` sends `File` uncompressed (clients' "send uncompressed"). Pictures go as
//! `Image` (suffixes per `guess_msgtype_from_path_suffix`), so core recodes per media quality and
//! sends `image_<date>.jpg`. Both pinned by `deltachat-jsonrpc/tests/real_server.rs`.
//!
//! `sys.msgsize_max_recommended` (`RECOMMENDED_FILE_SIZE`, ~22 MB, every relay) warns, never
//! caps: relays take more (issue #101: 200 MB); Desktop and iOS ignore it. A refusing relay fails
//! that message. Pictures never warn: core shrinks them.

use deltachat_jsonrpc::RpcClient;

/// Config key: core's recommended attachment size, bytes.
const LIMIT_KEY: &str = "sys.msgsize_max_recommended";

/// Suffixes core recodes; `heic`, `tiff`, `avif` are plain files to it, `gif` recoding spoils.
const PICTURES: [&str; 5] = ["jpg", "jpeg", "jpe", "png", "webp"];

/// Viewtype for core; `None` to let it decide.
pub(crate) fn outgoing_viewtype(path: &str) -> Option<&'static str> {
    is_picture(path).then_some("Image")
}

/// Core recodes this file?
pub(crate) fn is_picture(path: &str) -> bool {
    let suffix = std::path::Path::new(path)
        .extension()
        .map(|suffix| suffix.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    PICTURES.contains(&suffix.as_str())
}

/// File size; 0 if unmeasurable.
pub(crate) fn file_bytes(path: &str) -> u64 {
    if path.is_empty() {
        return 0;
    }
    std::fs::metadata(path).map_or(0, |file| file.len())
}

/// Over core's recommendation, maybe some relays' limit; warning only. False for pictures,
/// unknown limit (0) or unmeasurable file.
pub(crate) fn exceeds_limit(path: &str, limit: u64) -> bool {
    if limit == 0 || is_picture(path) {
        return false;
    }
    file_bytes(path) > limit
}

/// Recommended bytes; 0 if core unsaid.
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
            // Video, sound, app: core reads the suffix itself.
            "video-20260904-151212.mp4",
            "voice-20260904-151212.ogg",
            "game.xdc",
            // Images core does not recode, animation included.
            "party.gif",
            "photo.heic",
            "scan.tiff",
            // No suffix.
            "/home/user/Documents/notes",
            "",
        ] {
            assert!(!is_picture(path), "{path}");
            assert_eq!(outgoing_viewtype(path), None, "{path}");
        }
    }

    #[test]
    fn a_file_over_the_recommendation_is_large() {
        let dir = std::env::temp_dir().join(format!("piirit-media-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let video = dir.join("video.mp4");
        std::fs::write(&video, vec![0_u8; 2048]).expect("write the video");
        let path = video.to_string_lossy().into_owned();

        assert_eq!(file_bytes(&path), 2048);
        assert!(exceeds_limit(&path, 1024));
        assert!(!exceeds_limit(&path, 2048), "the limit itself still fits");
        assert!(!exceeds_limit(&path, 4096));
        // Limit unknown: nothing refused.
        assert!(!exceeds_limit(&path, 0));

        // Same-size picture: core shrinks it.
        let picture = dir.join("photo.jpg");
        std::fs::write(&picture, vec![0_u8; 2048]).expect("write the picture");
        assert!(!exceeds_limit(&picture.to_string_lossy(), 1024));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_not_there_is_not_large() {
        assert_eq!(file_bytes("/nowhere/at/all/video.mp4"), 0);
        assert_eq!(file_bytes(""), 0);
        assert!(!exceeds_limit("/nowhere/at/all/video.mp4", 1024));
        assert!(!exceeds_limit("", 1024));
    }
}
