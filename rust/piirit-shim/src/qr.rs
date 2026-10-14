//! QR codes: invite drawn out; camera codes read back.
//!
//! No protocol here: code carries core's invite link; read text goes to `check_qr` first, as
//! pasted links do. Frames are files: `grabToImage` is Qt 5.6 QML's only viewfinder pixel route;
//! `.pgm` path makes Qt write plain PGM, so no image crate. Both crates pure Rust, under the 1.75
//! floor.

use std::cell::RefCell;

use qmetaobject::*;

/// Module rows: `1` dark, `0` light, newline-joined.
pub(crate) fn encode_modules(text: &str) -> Result<(usize, String), String> {
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|err| err.to_string())?;
    let size = code.width();
    let colors = code.to_colors();
    let mut rows = String::with_capacity(size.saturating_mul(size.saturating_add(1)));
    for (index, color) in colors.iter().enumerate() {
        if index > 0 && index.checked_rem(size) == Some(0) {
            rows.push('\n');
        }
        rows.push(match color {
            qrcode::Color::Dark => '1',
            qrcode::Color::Light => '0',
        });
    }
    Ok((size, rows))
}

/// Greyscale: w, h, one byte per pixel, 0 black.
pub(crate) struct Grey {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// Binary PGM (`P5`) or PPM (`P6`); colour averaged.
pub(crate) fn parse_pnm(bytes: &[u8]) -> Option<Grey> {
    let (tokens, at) = pnm_header(bytes)?;
    let [magic, width, height, maximum] = tokens.as_slice() else {
        return None;
    };
    let width: usize = width.parse().ok()?;
    let height: usize = height.parse().ok()?;
    let maximum: u32 = maximum.parse().ok()?;
    if width == 0 || height == 0 || maximum == 0 || maximum > 255 {
        return None;
    }
    let count = width.checked_mul(height)?;
    let data = bytes.get(at..)?;
    let pixels = match magic.as_str() {
        "P5" => data.get(..count)?.to_vec(),
        "P6" => data
            .get(..count.checked_mul(3)?)?
            .chunks_exact(3)
            .map(|rgb| {
                let sum: u32 = rgb.iter().copied().map(u32::from).sum();
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

/// Header tokens (magic, width, height, max) and data offset. Whitespace-separated, `#` comments
/// allowed; exactly one whitespace byte between max and data.
fn pnm_header(bytes: &[u8]) -> Option<(Vec<String>, usize)> {
    let mut at = 0;
    let mut tokens: Vec<String> = Vec::new();
    while tokens.len() < 4 {
        let byte = *bytes.get(at)?;
        if byte == b'#' {
            at = end_of_line(bytes, at);
        } else if byte.is_ascii_whitespace() {
            at = at.saturating_add(1);
        } else {
            let end = end_of_token(bytes, at);
            tokens.push(String::from_utf8_lossy(bytes.get(at..end)?).into_owned());
            at = end;
        }
    }
    // `at` in the data: no overflow.
    Some((tokens, at.saturating_add(1)))
}

/// End of line at `at`, or end of data.
fn end_of_line(bytes: &[u8], at: usize) -> usize {
    let rest = bytes.get(at..).unwrap_or_default();
    at.saturating_add(rest.iter().take_while(|byte| **byte != b'\n').count())
}

/// End of token at `at`, or end of data.
fn end_of_token(bytes: &[u8], at: usize) -> usize {
    let rest = bytes.get(at..).unwrap_or_default();
    at.saturating_add(
        rest.iter()
            .take_while(|byte| !byte.is_ascii_whitespace())
            .count(),
    )
}

/// Text of first code in image, if any.
pub(crate) fn decode_grey(grey: &Grey) -> Option<String> {
    let mut prepared =
        rqrr::PreparedImage::prepare_from_greyscale(grey.width, grey.height, |x, y| {
            y.checked_mul(grey.width)
                .and_then(|row| row.checked_add(x))
                .and_then(|at| grey.pixels.get(at))
                .copied()
                .unwrap_or(u8::MAX)
        });
    prepared
        .detect_grids()
        .iter()
        .find_map(|grid| grid.decode().ok().map(|(_, content)| content))
}

/// File in app cache dir (inside sandbox grant): viewfinder frames, shown codes, captures
/// (`capture.rs`). `name` may have a directory, created too.
pub(crate) fn cache_file(name: &str) -> Result<std::path::PathBuf, String> {
    let base = std::env::var("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
        })
        .map_err(|err| format!("neither XDG_CACHE_HOME nor HOME is set: {err}"))?;
    let path = base.join("piirit/piirit").join(name);
    let dir = path.parent().unwrap_or(&path);
    std::fs::create_dir_all(dir)
        .map_err(|err| format!("cannot create cache dir {}: {err}", dir.display()))?;
    Ok(path)
}

/// Pixels per module: square modules at any scale.
const MODULE_PIXELS: usize = 8;
/// Quiet-zone modules, as the standard asks.
const QUIET_ZONE: usize = 4;

/// Binary PGM with quiet zone, dark modules black on white. Qt reads PGM, so `Image` shows it
/// and reloads from file when textures are lost (app backgrounded).
pub(crate) fn write_pgm(size: usize, modules: &str, path: &std::path::Path) -> Result<(), String> {
    let too_large = || format!("a code of {size} modules is too large to draw");
    let edge = size
        .checked_add(2 * QUIET_ZONE)
        .and_then(|modules| modules.checked_mul(MODULE_PIXELS))
        .ok_or_else(too_large)?;
    // Row of modules: `MODULE_PIXELS` lines.
    let band = edge.checked_mul(MODULE_PIXELS).ok_or_else(too_large)?;
    let mut pixels = vec![u8::MAX; edge.checked_mul(edge).ok_or_else(too_large)?];
    // Bands and cells skip quiet zone top/left: no arithmetic placement, nothing lands outside.
    let bands = pixels.chunks_exact_mut(band).skip(QUIET_ZONE);
    for (band, row) in bands.zip(modules.lines()) {
        for line in band.chunks_exact_mut(edge) {
            let cells = line.chunks_exact_mut(MODULE_PIXELS).skip(QUIET_ZONE);
            for (cell, module) in cells.zip(row.chars()) {
                if module == '1' {
                    cell.fill(0);
                }
            }
        }
    }
    let mut bytes = format!("P5\n{edge} {edge}\n255\n").into_bytes();
    bytes.extend_from_slice(&pixels);
    std::fs::write(path, bytes).map_err(|err| format!("cannot write {}: {err}", path.display()))
}

/// Name per text: different texts, different files.
fn file_stem(text: &str) -> String {
    // FNV-1a 64-bit: not security.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("qr-{hash:016x}")
}

/// QR code: modules, and a picture file.
///
/// ```qml
/// QrCode { id: qr; text: page.myInvite }
/// Image { source: qr.image; smooth: false; visible: qr.size > 0 }
/// ```
#[derive(QObject, Default)]
pub struct QrCode {
    base: qt_base_class!(trait QObject),

    /// Text carried; setting re-encodes.
    pub text: qt_property!(QString; WRITE set_text NOTIFY modules_changed),
    /// Module rows; empty if none.
    pub modules: qt_property!(QString; NOTIFY modules_changed),
    /// Modules per side; 0 if no code.
    pub size: qt_property!(u32; NOTIFY modules_changed),
    /// `file://` URL of the PGM; empty if none.
    pub image: qt_property!(QString; NOTIFY modules_changed),
    /// Emitted after every re-encode.
    pub modules_changed: qt_signal!(),

    /// File `image` names.
    shown: Option<std::path::PathBuf>,
}

/// Code files shown, one entry per showing [`QrCode`]: two pages may share one file.
///
/// Second-device code is new per offer, so cache would keep every code. Shown files stay
/// (`Image` reloads on texture loss); other code files go as next is written.
static SHOWN: std::sync::Mutex<Vec<std::path::PathBuf>> = std::sync::Mutex::new(Vec::new());

/// [`SHOWN`], even after a panic: at worst a file kept.
fn shown_codes() -> std::sync::MutexGuard<'static, Vec<std::path::PathBuf>> {
    SHOWN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl QrCode {
    /// Set text and re-encode.
    pub fn set_text(&mut self, text: QString) {
        self.text = text;
        let text = self.text.to_string();
        let (size, modules) = if text.is_empty() {
            (0, String::new())
        } else {
            encode_modules(&text).unwrap_or((0, String::new()))
        };
        self.size = u32::try_from(size).unwrap_or(0);
        self.stop_showing();
        self.image = if size == 0 {
            QString::default()
        } else {
            Self::write_image(&text, size, &modules)
                .map(|path| self.show(path))
                .unwrap_or_default()
        };
        self.modules = modules.into();
        self.modules_changed();
    }

    /// Show file at `path`, remove unshown codes, return URL.
    fn show(&mut self, path: std::path::PathBuf) -> QString {
        let url = format!("file://{}", path.to_string_lossy());
        let mut list = shown_codes();
        list.push(path.clone());
        if let Some(dir) = path.parent() {
            remove_codes_not_shown(dir, &list);
        }
        self.shown = Some(path);
        url.into()
    }

    /// The code as a file.
    fn write_image(text: &str, size: usize, modules: &str) -> Result<std::path::PathBuf, String> {
        let path = cache_file(&format!("{}.pgm", file_stem(text)))?;
        write_pgm(size, modules, &path)?;
        Ok(path)
    }

    /// Drop this code's file from [`SHOWN`].
    fn stop_showing(&mut self) {
        let Some(path) = self.shown.take() else {
            return;
        };
        let mut list = shown_codes();
        if let Some(index) = list.iter().position(|other| *other == path) {
            list.swap_remove(index);
        }
    }
}

impl Drop for QrCode {
    fn drop(&mut self) {
        self.stop_showing();
    }
}

/// Is `name` a code file ([`file_stem`])? Not the frame.
fn is_code_file(name: &str) -> bool {
    name.strip_prefix("qr-")
        .and_then(|rest| rest.strip_suffix(".pgm"))
        .is_some_and(|hash| hash.len() == 16 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// Remove code files in `dir` not in `keep`.
fn remove_codes_not_shown(dir: &std::path::Path, keep: &[std::path::PathBuf]) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_code = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_code_file);
        if is_code && !keep.contains(&path) {
            if let Err(err) = std::fs::remove_file(&path) {
                eprintln!("piirit: removing old code {}: {err}", path.display());
            }
        }
    }
}

/// Reads QR codes from viewfinder frames.
///
/// ```qml
/// QrScanner { id: scanner; onFound: page.scanned(text) }
/// // ... viewfinder.grabToImage(function(r) {
/// //         r.saveToFile(scanner.frame_path()); scanner.decode(scanner.frame_path()) }, ...)
/// ```
#[derive(QObject, Default)]
pub struct QrScanner {
    base: qt_base_class!(trait QObject),

    /// True from `decode` until answered; page grabs next frame only after, so none queue.
    pub busy: qt_property!(bool; NOTIFY busy_changed),
    /// Emitted when `busy` changes.
    pub busy_changed: qt_signal!(),

    /// Frame file for `decode`.
    pub frame_path: qt_method!(fn(&mut self) -> QString),
    /// Read `path` for a code; answers `found` or `nothing`, off Qt thread.
    pub decode: qt_method!(fn(&mut self, path: QString)),
    /// Code read: text.
    pub found: qt_signal!(text: QString),
    /// No code, or unreadable.
    pub nothing: qt_signal!(),
    /// No frame file: scanning cannot work.
    pub error: qt_signal!(message: QString),

    /// Path, once worked out.
    path: RefCell<Option<String>>,
}

impl QrScanner {
    /// Frame file for `decode`.
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

    /// Read `path`, look for a code.
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
        // Own thread, not core runtime: decoding needs no core; frames arrive without one.
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

    /// Code as greyscale, `scale` pixels per module.
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

    /// P5 file as Qt writes it.
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
        // Different texts, different files; same text, same file.
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
        // 2x1 P6 with a comment: black and white, read as grey.
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
        encode_modules("").unwrap();
    }

    /// Removes old codes; keeps shown ones and the frame.
    #[test]
    fn old_codes_go_and_shown_ones_and_the_frame_stay() {
        let temp = std::env::temp_dir().join(format!("piirit-qr-old-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).expect("create temp dir");
        let old = temp.join(format!("{}.pgm", file_stem("an offer from before")));
        let other_page = temp.join(format!("{}.pgm", file_stem("my invite")));
        let new = temp.join(format!("{}.pgm", file_stem("the offer now")));
        let frame = temp.join("qr-frame.pgm");
        let unrelated = temp.join("photo.jpg");
        for path in [&old, &other_page, &new, &frame, &unrelated] {
            std::fs::write(path, b"P5").expect("write");
        }

        remove_codes_not_shown(&temp, &[other_page.clone(), new.clone()]);
        assert!(!old.exists(), "a code nothing shows any more was kept");
        assert!(other_page.exists(), "a code another page shows was removed");
        assert!(new.exists(), "the code just written was removed");
        assert!(frame.exists(), "the scanner's frame was taken for a code");
        assert!(unrelated.exists(), "a file that is no code was removed");

        assert!(is_code_file("qr-0123456789abcdef.pgm"));
        assert!(!is_code_file("qr-frame.pgm"));
        assert!(!is_code_file("qr-0123456789abcdef.png"));
        assert!(!is_code_file("qr-0123456789abcde.pgm"));
        let _ = std::fs::remove_dir_all(&temp);
    }
}
