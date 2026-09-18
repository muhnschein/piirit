//! QR codes both ways: the account's own invite drawn as one, and one held
//! up to the camera read back into the text it carries.
//!
//! Neither side touches the protocol. What goes into a code is the invite
//! link the core hands out, and what comes out of one is text the core is
//! asked to classify (`check_qr`) before anything is done with it -- the
//! same path a pasted link takes. The two crates are pure Rust and under
//! the 1.75 floor.
//!
//! The camera frames arrive as files. `Item.grabToImage` is the only way
//! QML on Qt 5.6 hands a viewfinder's pixels to anything, and what it
//! hands over is saved to a path; `.pgm` makes Qt write a plain greyscale
//! PGM, which is a header and the bytes, so no image crate is needed to
//! read it back.

use std::cell::RefCell;

use qmetaobject::*;

/// The modules of a code as rows of `1` (dark) and `0` (light), for a
/// Canvas to draw.
///
/// Rows are joined with newlines; every row is `size` long.
pub(crate) fn encode_modules(text: &str) -> Result<(usize, String), String> {
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|err| err.to_string())?;
    let size = code.width();
    let colors = code.to_colors();
    let mut rows = String::with_capacity(size * (size + 1));
    for (index, color) in colors.iter().enumerate() {
        if index > 0 && index % size == 0 {
            rows.push('\n');
        }
        rows.push(match color {
            qrcode::Color::Dark => '1',
            qrcode::Color::Light => '0',
        });
    }
    Ok((size, rows))
}

/// A greyscale image: width, height, and one byte per pixel, 0 black.
pub(crate) struct Grey {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// Read a binary PGM (`P5`) or PPM (`P6`), which is what Qt writes for a
/// `.pgm` or `.ppm` path. Colour is averaged down to grey.
pub(crate) fn parse_pnm(bytes: &[u8]) -> Option<Grey> {
    let (tokens, at) = pnm_header(bytes)?;
    let width: usize = tokens[1].parse().ok()?;
    let height: usize = tokens[2].parse().ok()?;
    let maximum: u32 = tokens[3].parse().ok()?;
    if width == 0 || height == 0 || maximum == 0 || maximum > 255 {
        return None;
    }
    let count = width.checked_mul(height)?;
    let data = bytes.get(at..)?;
    let pixels = match tokens[0].as_str() {
        "P5" => data.get(..count)?.to_vec(),
        "P6" => data
            .get(..count.checked_mul(3)?)?
            .chunks_exact(3)
            .map(|rgb| {
                let sum = u32::from(rgb[0]) + u32::from(rgb[1]) + u32::from(rgb[2]);
                u8::try_from(sum / 3).unwrap_or(u8::MAX)
            })
            .collect(),
        _ => return None,
    };
    Some(Grey {
        width,
        height,
        pixels,
    })
}

/// The header's four tokens -- magic, width, height, maximum -- and the
/// offset of the first pixel byte.
///
/// The tokens are separated by whitespace, with `#` comments allowed
/// between them, and exactly one whitespace byte separates the maximum
/// from the data.
fn pnm_header(bytes: &[u8]) -> Option<(Vec<String>, usize)> {
    let mut at = 0;
    let mut tokens: Vec<String> = Vec::new();
    while tokens.len() < 4 && at < bytes.len() {
        let byte = bytes[at];
        if byte == b'#' {
            at = end_of_line(bytes, at);
        } else if byte.is_ascii_whitespace() {
            at += 1;
        } else {
            let end = end_of_token(bytes, at);
            tokens.push(String::from_utf8_lossy(&bytes[at..end]).into_owned());
            at = end;
        }
    }
    if tokens.len() < 4 {
        return None;
    }
    Some((tokens, at + 1))
}

/// Where the line starting at `at` ends: its newline, which is left to be
/// read as whitespace, or the end of the data.
fn end_of_line(bytes: &[u8], at: usize) -> usize {
    at + bytes[at..]
        .iter()
        .take_while(|byte| **byte != b'\n')
        .count()
}

/// Where the token starting at `at` ends: the first whitespace byte, or
/// the end of the data.
fn end_of_token(bytes: &[u8], at: usize) -> usize {
    at + bytes[at..]
        .iter()
        .take_while(|byte| !byte.is_ascii_whitespace())
        .count()
}

/// The text of the first code found in the image, if any.
pub(crate) fn decode_grey(grey: &Grey) -> Option<String> {
    let mut prepared =
        rqrr::PreparedImage::prepare_from_greyscale(grey.width, grey.height, |x, y| {
            grey.pixels
                .get(y * grey.width + x)
                .copied()
                .unwrap_or(u8::MAX)
        });
    prepared
        .detect_grids()
        .iter()
        .find_map(|grid| grid.decode().ok().map(|(_, content)| content))
}

/// A file of the given name in the app's own cache directory, which is
/// inside the sandbox grant: where a viewfinder frame goes for decoding,
/// where a code goes to be shown, and where a capture waits to be sent
/// (`capture.rs`). `name` may carry a directory, which is made too.
pub(crate) fn cache_file(name: &str) -> Result<std::path::PathBuf, String> {
    let base = std::env::var("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
        })
        .map_err(|_| "neither XDG_CACHE_HOME nor HOME is set".to_string())?;
    let path = base.join("piirit/piirit").join(name);
    let dir = path.parent().unwrap_or(&path);
    std::fs::create_dir_all(dir)
        .map_err(|err| format!("cannot create cache dir {}: {err}", dir.display()))?;
    Ok(path)
}

/// Pixels per module in the file a code is shown from. Enough that a
/// phone held up to the screen sees square modules whatever the scaling.
const MODULE_PIXELS: usize = 8;
/// Modules of quiet zone around a shown code, as the standard asks.
const QUIET_ZONE: usize = 4;

/// A code written as a binary PGM, with its quiet zone, dark modules
/// black on white. Qt reads PGM itself, so an `Image` shows it, and it is
/// reloaded from the file when the window's textures are lost -- which is
/// what happens to an app put in the background.
pub(crate) fn write_pgm(size: usize, modules: &str, path: &std::path::Path) -> Result<(), String> {
    let edge = (size + 2 * QUIET_ZONE) * MODULE_PIXELS;
    let mut pixels = vec![u8::MAX; edge * edge];
    for (row, line) in modules.lines().enumerate() {
        for (column, module) in line.chars().enumerate() {
            if module != '1' {
                continue;
            }
            let left = (column + QUIET_ZONE) * MODULE_PIXELS;
            for dy in 0..MODULE_PIXELS {
                let top = ((row + QUIET_ZONE) * MODULE_PIXELS + dy) * edge;
                pixels[top + left..top + left + MODULE_PIXELS].fill(0);
            }
        }
    }
    let mut bytes = format!("P5\n{edge} {edge}\n255\n").into_bytes();
    bytes.extend_from_slice(&pixels);
    std::fs::write(path, bytes).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

/// A short, stable name for a text: two codes with different texts get
/// different files, so a stale one is never shown for the other.
fn file_stem(text: &str) -> String {
    // FNV-1a, 64 bits: not for security, only for telling texts apart.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("qr-{hash:016x}")
}

/// A QR code of some text: as modules, and as a picture file for an
/// `Image` to show.
///
/// ```qml
/// QrCode { id: qr; text: page.myInvite }
/// Image { source: qr.image; smooth: false; visible: qr.size > 0 }
/// ```
#[derive(QObject, Default)]
pub struct QrCode {
    base: qt_base_class!(trait QObject),

    /// What the code carries. Setting it re-encodes.
    pub text: qt_property!(QString; WRITE set_text NOTIFY modules_changed),
    /// Rows of `1` and `0`, newline-separated, `size` of each. Empty when
    /// there is no text, or it could not be encoded.
    pub modules: qt_property!(QString; NOTIFY modules_changed),
    /// Modules per side, 0 when there is no code.
    pub size: qt_property!(u32; NOTIFY modules_changed),
    /// A `file://` URL of the code drawn as a PGM in the cache directory,
    /// with its quiet zone. Empty when there is no code, or the file
    /// could not be written.
    pub image: qt_property!(QString; NOTIFY modules_changed),
    /// Emitted after every re-encode.
    pub modules_changed: qt_signal!(),
}

impl QrCode {
    /// Set the text and re-encode.
    pub fn set_text(&mut self, text: QString) {
        self.text = text;
        let text = self.text.to_string();
        let (size, modules) = if text.is_empty() {
            (0, String::new())
        } else {
            encode_modules(&text).unwrap_or((0, String::new()))
        };
        self.size = u32::try_from(size).unwrap_or(0);
        self.image = if size == 0 {
            QString::default()
        } else {
            Self::write_image(&text, size, &modules)
                .unwrap_or_default()
                .into()
        };
        self.modules = modules.into();
        self.modules_changed();
    }

    /// The code as a file, and its URL.
    fn write_image(text: &str, size: usize, modules: &str) -> Result<String, String> {
        let path = cache_file(&format!("{}.pgm", file_stem(text)))?;
        write_pgm(size, modules, &path)?;
        Ok(format!("file://{}", path.to_string_lossy()))
    }
}

/// Reads QR codes out of viewfinder frames.
///
/// ```qml
/// QrScanner { id: scanner; onFound: page.scanned(text) }
/// // ... viewfinder.grabToImage(function(r) {
/// //         r.saveToFile(scanner.frame_path()); scanner.decode(scanner.frame_path()) }, ...)
/// ```
#[derive(QObject, Default)]
pub struct QrScanner {
    base: qt_base_class!(trait QObject),

    /// True from `decode` until it answers. The page grabs the next frame
    /// only once this is down, so frames never queue up behind a slow
    /// decode.
    pub busy: qt_property!(bool; NOTIFY busy_changed),
    /// Emitted when `busy` changes.
    pub busy_changed: qt_signal!(),

    /// Where to write a frame for `decode`. Created on first use.
    pub frame_path: qt_method!(fn(&mut self) -> QString),
    /// Read the image at `path` and look for a code. Answers on `found`
    /// or `nothing`, off the Qt thread.
    pub decode: qt_method!(fn(&mut self, path: QString)),
    /// A code was read: its text.
    pub found: qt_signal!(text: QString),
    /// The frame held no code, or could not be read.
    pub nothing: qt_signal!(),
    /// The frame file could not be placed. The page cannot scan without it.
    pub error: qt_signal!(message: QString),

    /// The path, once worked out.
    path: RefCell<Option<String>>,
}

impl QrScanner {
    /// Where to write a frame for `decode`.
    pub fn frame_path(&mut self) -> QString {
        if let Some(path) = self.path.borrow().as_ref() {
            return path.as_str().into();
        }
        match cache_file("qr-frame.pgm") {
            Ok(path) => {
                let path = path.to_string_lossy().into_owned();
                *self.path.borrow_mut() = Some(path.clone());
                path.into()
            }
            Err(err) => {
                self.error(err.into());
                QString::default()
            }
        }
    }

    /// Read the image at `path` and look for a code.
    pub fn decode(&mut self, path: QString) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.busy_changed();
        let ptr: QPointer<Self> = QPointer::from(&*self);
        let done = queued_callback(move |result: Option<String>| {
            let Some(this) = ptr.as_pinned() else { return };
            this.borrow_mut().busy = false;
            this.borrow().busy_changed();
            match result {
                Some(text) => this.borrow().found(text.into()),
                None => this.borrow().nothing(),
            }
        });
        let path = path.to_string();
        // A thread of its own rather than the core's runtime: decoding
        // needs no core, and a frame arrives whether or not one is up.
        std::thread::spawn(move || {
            let result = std::fs::read(&path)
                .ok()
                .and_then(|bytes| parse_pnm(&bytes))
                .and_then(|grey| decode_grey(&grey));
            done(result);
        });
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// A code drawn as a greyscale image, with a quiet zone and each
    /// module `scale` pixels wide: what a camera would see, less the
    /// camera.
    fn rasterize(size: usize, modules: &str, scale: usize) -> Grey {
        let quiet = 4;
        let edge = (size + 2 * quiet) * scale;
        let mut pixels = vec![u8::MAX; edge * edge];
        for (row, line) in modules.lines().enumerate() {
            for (column, module) in line.chars().enumerate() {
                if module != '1' {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = (column + quiet) * scale + dx;
                        let y = (row + quiet) * scale + dy;
                        pixels[y * edge + x] = 0;
                    }
                }
            }
        }
        Grey {
            width: edge,
            height: edge,
            pixels,
        }
    }

    /// A P5 file, as Qt writes one for a `.pgm` path.
    fn pgm(grey: &Grey) -> Vec<u8> {
        let mut bytes = format!("P5\n{} {}\n255\n", grey.width, grey.height).into_bytes();
        bytes.extend_from_slice(&grey.pixels);
        bytes
    }

    #[test]
    fn an_invite_survives_the_round_trip_through_pixels() {
        let invite = "https://i.delta.chat/#ABCDEF0123456789&a=me%40example.org&n=Me";
        let (size, modules) = encode_modules(invite).expect("encode");
        assert!(size >= 21, "a code is at least version 1: {size}");
        assert_eq!(modules.lines().count(), size);
        assert!(modules.lines().all(|row| row.len() == size));

        let grey = rasterize(size, &modules, 4);
        let file = pgm(&grey);
        let read = parse_pnm(&file).expect("parse what was written");
        assert_eq!((read.width, read.height), (grey.width, grey.height));
        assert_eq!(decode_grey(&read).as_deref(), Some(invite));
    }

    #[test]
    fn the_file_written_for_showing_reads_back_as_the_text() {
        let temp = std::env::temp_dir().join(format!("piirit-qr-file-{}", std::process::id()));
        std::fs::create_dir_all(&temp).expect("create temp dir");
        let text = "https://i.delta.chat/#FEDCBA9876543210&a=me%40example.org&n=Me";
        let (size, modules) = encode_modules(text).expect("encode");
        let path = temp.join("shown.pgm");
        write_pgm(size, &modules, &path).expect("write");
        let read = parse_pnm(&std::fs::read(&path).expect("read back")).expect("parse");
        assert_eq!(read.width, (size + 2 * QUIET_ZONE) * MODULE_PIXELS);
        assert_eq!(decode_grey(&read).as_deref(), Some(text));
        // Different texts, different files; the same text, the same file.
        assert_ne!(file_stem(text), file_stem("dcaccount:nine.testrun.org"));
        assert_eq!(file_stem(text), file_stem(text));
    }

    #[test]
    fn a_provider_payload_round_trips_too() {
        let payload = "dcaccount:https://nine.testrun.org/new";
        let (size, modules) = encode_modules(payload).expect("encode");
        let grey = rasterize(size, &modules, 3);
        assert_eq!(decode_grey(&grey).as_deref(), Some(payload));
    }

    #[test]
    fn colour_is_averaged_and_comments_are_skipped() {
        // A 2x1 P6 with a comment: black and white, read as grey.
        let file = b"P6\n# a comment\n2 1\n255\n\x00\x00\x00\xff\xff\xff".to_vec();
        let grey = parse_pnm(&file).expect("parse");
        assert_eq!((grey.width, grey.height), (2, 1));
        assert_eq!(grey.pixels, vec![0, 255]);
    }

    #[test]
    fn a_frame_without_a_code_is_nothing_and_junk_is_not_a_frame() {
        let blank = Grey {
            width: 64,
            height: 64,
            pixels: vec![u8::MAX; 64 * 64],
        };
        assert_eq!(decode_grey(&blank), None);
        assert!(parse_pnm(b"not an image").is_none());
        assert!(parse_pnm(b"P5\n4 4\n255\nshort").is_none());
        assert!(encode_modules("").is_ok());
    }
}
