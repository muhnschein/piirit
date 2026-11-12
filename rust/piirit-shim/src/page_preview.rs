//! Office file's own first-page picture, as its writer stored it: ODF
//! `Thumbnails/thumbnail.png` (always, from `LibreOffice`), OOXML `docProps/thumbnail.*` (some
//! writers). Bytes only; Qt decodes (`pictures.rs`, kind `page`).
//!
//! Zip read here, not a zip crate: one named entry, stored or deflated, is all needed
//! (`zip` 2.x pulls deps past Rust 1.75). Peer's file: every size capped, every offset checked.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Where writers put it, most wanted first. EMF/WMF left out: Qt can't draw them.
const NAMES: [&str; 4] = [
    "Thumbnails/thumbnail.png",
    "docProps/thumbnail.jpeg",
    "docProps/thumbnail.jpg",
    "docProps/thumbnail.png",
];

/// Thumbnail bigger than this is no thumbnail.
const MAX_PICTURE: usize = 4 << 20;
/// Central directory past this: not an office file worth reading.
const MAX_DIRECTORY: usize = 1 << 20;
/// End record (22) plus longest comment.
const MAX_TAIL: u64 = 22 + 0xFFFF;

const END_RECORD: u32 = 0x0605_4b50;
const DIRECTORY_ENTRY: u32 = 0x0201_4b50;
const LOCAL_HEADER: u32 = 0x0403_4b50;

/// `path`'s stored first-page picture (PNG or JPEG bytes); `None` if not a zip, none stored,
/// encrypted, or past a cap.
#[must_use]
pub fn embedded(path: &Path) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    let tail_length = length.min(MAX_TAIL);
    let tail = read_at(&mut file, length.checked_sub(tail_length)?, tail_length)?;
    let (directory_at, directory_length) = end_record(&tail)?;
    if directory_length > MAX_DIRECTORY {
        return None;
    }
    let directory = read_at(&mut file, directory_at, directory_length as u64)?;
    let entry = NAMES.iter().find_map(|name| find_entry(&directory, name))?;
    let header = read_at(&mut file, entry.header_at, 30)?;
    if u32_at(&header, 0)? != LOCAL_HEADER {
        return None;
    }
    let data_at = entry
        .header_at
        .checked_add(30)?
        .checked_add(u64::from(u16_at(&header, 26)?))?
        .checked_add(u64::from(u16_at(&header, 28)?))?;
    let packed = read_at(&mut file, data_at, entry.packed as u64)?;
    unpack(&packed, entry.method, entry.size)
}

/// Directory entry of the wanted picture.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    method: u16,
    packed: usize,
    size: usize,
    header_at: u64,
}

/// Central directory's offset and length from the end record in `tail`. Record's comment
/// must run to file's end: signature inside a comment is not one.
fn end_record(tail: &[u8]) -> Option<(u64, usize)> {
    let last = tail.len().checked_sub(22)?;
    let at = (0..=last).rev().find(|&at| {
        u32_at(tail, at) == Some(END_RECORD)
            && u16_at(tail, at.saturating_add(20)).map(usize::from) == Some(last.saturating_sub(at))
    })?;
    let length = u32_at(tail, at.checked_add(12)?)?;
    let offset = u32_at(tail, at.checked_add(16)?)?;
    Some((u64::from(offset), usize::try_from(length).ok()?))
}

/// `name`'s entry in `directory`, if there, unencrypted, within caps.
fn find_entry(directory: &[u8], name: &str) -> Option<Entry> {
    let mut at = 0_usize;
    while u32_at(directory, at) == Some(DIRECTORY_ENTRY) {
        let flags = u16_at(directory, at.checked_add(8)?)?;
        let method = u16_at(directory, at.checked_add(10)?)?;
        let packed = u32_at(directory, at.checked_add(20)?)?;
        let size = u32_at(directory, at.checked_add(24)?)?;
        let name_length = usize::from(u16_at(directory, at.checked_add(28)?)?);
        let extra_length = usize::from(u16_at(directory, at.checked_add(30)?)?);
        let comment_length = usize::from(u16_at(directory, at.checked_add(32)?)?);
        let header_at = u32_at(directory, at.checked_add(42)?)?;
        let name_at = at.checked_add(46)?;
        let entry_name = directory.get(name_at..name_at.checked_add(name_length)?)?;
        if entry_name == name.as_bytes() {
            let packed = usize::try_from(packed).ok()?;
            let size = usize::try_from(size).ok()?;
            // Bit 0: encrypted.
            if flags & 1 != 0 || packed > MAX_PICTURE {
                return None;
            }
            return Some(Entry {
                method,
                packed,
                size,
                header_at: u64::from(header_at),
            });
        }
        at = name_at
            .checked_add(name_length)?
            .checked_add(extra_length)?
            .checked_add(comment_length)?;
    }
    None
}

/// Stored or deflated bytes, exactly `size` long, at most [`MAX_PICTURE`]; anything else `None`.
fn unpack(packed: &[u8], method: u16, size: usize) -> Option<Vec<u8>> {
    let bytes = match method {
        0 => packed.to_vec(),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(packed, MAX_PICTURE).ok()?,
        _ => return None,
    };
    (bytes.len() == size && !bytes.is_empty()).then_some(bytes)
}

/// `length` bytes at `at`; `None` if the file is shorter.
fn read_at(file: &mut File, at: u64, length: u64) -> Option<Vec<u8>> {
    file.seek(SeekFrom::Start(at)).ok()?;
    let mut bytes = Vec::new();
    file.take(length).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 == length).then_some(bytes)
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let pair = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes(pair.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let quad = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes(quad.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// One zip entry to write: name, method (0 or 8), flags, data as stored.
    struct Part<'a> {
        name: &'a str,
        method: u16,
        flags: u16,
        size: usize,
        data: Vec<u8>,
    }

    fn stored<'a>(name: &'a str, bytes: &[u8]) -> Part<'a> {
        Part {
            name,
            method: 0,
            flags: 0,
            size: bytes.len(),
            data: bytes.to_vec(),
        }
    }

    fn deflated<'a>(name: &'a str, bytes: &[u8]) -> Part<'a> {
        Part {
            name,
            method: 8,
            flags: 0,
            size: bytes.len(),
            data: miniz_oxide::deflate::compress_to_vec(bytes, 6),
        }
    }

    /// Zip as writers lay it out: local headers, data, directory, end record, `comment`.
    fn zip(parts: &[Part], comment: &[u8]) -> Vec<u8> {
        let number = |value: usize| u32::try_from(value).expect("small");
        let mut out = Vec::new();
        let mut directory = Vec::new();
        for part in parts {
            let at = number(out.len());
            out.extend(LOCAL_HEADER.to_le_bytes());
            out.extend([20, 0]);
            out.extend(part.flags.to_le_bytes());
            out.extend(part.method.to_le_bytes());
            out.extend([0; 8]); // time, date, crc
            out.extend(number(part.data.len()).to_le_bytes());
            out.extend(number(part.size).to_le_bytes());
            out.extend(u16::try_from(part.name.len()).expect("short").to_le_bytes());
            out.extend(3_u16.to_le_bytes()); // extra
            out.extend(part.name.as_bytes());
            out.extend([0; 3]);
            out.extend(&part.data);

            directory.extend(DIRECTORY_ENTRY.to_le_bytes());
            directory.extend([20, 0, 20, 0]);
            directory.extend(part.flags.to_le_bytes());
            directory.extend(part.method.to_le_bytes());
            directory.extend([0; 8]);
            directory.extend(number(part.data.len()).to_le_bytes());
            directory.extend(number(part.size).to_le_bytes());
            directory.extend(u16::try_from(part.name.len()).expect("short").to_le_bytes());
            directory.extend(0_u16.to_le_bytes()); // extra
            directory.extend(2_u16.to_le_bytes()); // comment
            directory.extend([0; 8]); // disk, attributes
            directory.extend(at.to_le_bytes());
            directory.extend(part.name.as_bytes());
            directory.extend(b"hi");
        }
        let directory_at = number(out.len());
        out.extend(&directory);
        out.extend(END_RECORD.to_le_bytes());
        out.extend([0; 4]);
        let count = u16::try_from(parts.len()).expect("few");
        out.extend(count.to_le_bytes());
        out.extend(count.to_le_bytes());
        out.extend(number(directory.len()).to_le_bytes());
        out.extend(directory_at.to_le_bytes());
        out.extend(u16::try_from(comment.len()).expect("short").to_le_bytes());
        out.extend(comment);
        out
    }

    fn written(test: &str, bytes: &[u8]) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("piirit-page-preview-{}-{test}", std::process::id()));
        std::fs::write(&path, bytes).expect("temp file");
        path
    }

    fn read(test: &str, bytes: &[u8]) -> Option<Vec<u8>> {
        let path = written(test, bytes);
        let found = embedded(&path);
        std::fs::remove_file(&path).expect("removed");
        found
    }

    const PNG: &[u8] = b"\x89PNG picture bytes, repeated repeated repeated repeated";

    #[test]
    fn an_odf_thumbnail_comes_out_whether_stored_or_deflated() {
        let content = stored("content.xml", b"<office:document/>");
        let file = zip(&[content, stored("Thumbnails/thumbnail.png", PNG)], b"");
        assert_eq!(read("odf-stored", &file).as_deref(), Some(PNG));
        let content = deflated("content.xml", b"<office:document/>");
        let file = zip(&[content, deflated("Thumbnails/thumbnail.png", PNG)], b"");
        assert_eq!(read("odf-deflated", &file).as_deref(), Some(PNG));
    }

    #[test]
    fn an_ooxml_thumbnail_comes_out_and_odf_wins_if_both() {
        let file = zip(&[deflated("docProps/thumbnail.jpeg", b"jpeg")], b"");
        assert_eq!(read("ooxml", &file).as_deref(), Some(&b"jpeg"[..]));
        let file = zip(
            &[
                stored("docProps/thumbnail.jpeg", b"jpeg"),
                stored("Thumbnails/thumbnail.png", PNG),
            ],
            b"",
        );
        assert_eq!(read("both", &file).as_deref(), Some(PNG));
    }

    #[test]
    fn a_comment_holding_the_end_signature_does_not_mislead() {
        let mut comment = b"note ".to_vec();
        comment.extend(END_RECORD.to_le_bytes());
        comment.extend([0xEE; 30]);
        let file = zip(&[stored("Thumbnails/thumbnail.png", PNG)], &comment);
        assert_eq!(read("comment", &file).as_deref(), Some(PNG));
        let file = zip(&[stored("Thumbnails/thumbnail.png", PNG)], b"plain comment");
        assert_eq!(read("plain-comment", &file).as_deref(), Some(PNG));
    }

    #[test]
    fn no_picture_encrypted_or_odd_method_is_nothing() {
        let file = zip(&[stored("content.xml", b"x")], b"");
        assert_eq!(read("none", &file), None);
        let mut locked = stored("Thumbnails/thumbnail.png", PNG);
        locked.flags = 1;
        assert_eq!(read("encrypted", &zip(&[locked], b"")), None);
        let mut odd = stored("Thumbnails/thumbnail.png", PNG);
        odd.method = 12;
        assert_eq!(read("bzip2", &zip(&[odd], b"")), None);
        // EMF: Qt can't draw it.
        let file = zip(&[stored("docProps/thumbnail.emf", b"emf")], b"");
        assert_eq!(read("emf", &file), None);
    }

    #[test]
    fn a_size_that_lies_or_is_too_big_is_nothing() {
        let mut lying = stored("Thumbnails/thumbnail.png", PNG);
        lying.size = PNG.len() + 1;
        assert_eq!(read("lying", &zip(&[lying], b"")), None);
        let mut big = stored("Thumbnails/thumbnail.png", PNG);
        big.size = MAX_PICTURE + 1;
        assert_eq!(read("big", &zip(&[big], b"")), None);
        let empty = stored("Thumbnails/thumbnail.png", b"");
        assert_eq!(read("empty", &zip(&[empty], b"")), None);
    }

    #[test]
    fn a_pdf_a_short_file_and_a_cut_zip_are_nothing() {
        assert_eq!(
            read("pdf", b"%PDF-1.7 not a zip at all, long enough to look"),
            None
        );
        assert_eq!(read("short", b"PK"), None);
        assert_eq!(read("empty-file", b""), None);
        let file = zip(&[stored("Thumbnails/thumbnail.png", PNG)], b"");
        // Every cut: never a panic, never wrong bytes.
        for cut in 0..file.len() {
            let found = read("cut", file.get(..cut).expect("in range"));
            assert!(
                found.is_none() || found.as_deref() == Some(PNG),
                "cut at {cut}"
            );
        }
        assert_eq!(embedded(Path::new("/nonexistent/piirit/file.odt")), None);
    }

    #[test]
    fn directory_offsets_past_the_file_are_nothing() {
        let mut file = zip(&[stored("Thumbnails/thumbnail.png", PNG)], b"");
        let end = file.len() - 22;
        // Directory offset to far away.
        file.splice(end + 16..end + 20, 0x7FFF_FFFF_u32.to_le_bytes());
        assert_eq!(read("far", &file), None);
        let mut file = zip(&[stored("Thumbnails/thumbnail.png", PNG)], b"");
        file.splice(end + 16..end + 20, u32::MAX.to_le_bytes());
        assert_eq!(read("zip64", &file), None);
    }
}
