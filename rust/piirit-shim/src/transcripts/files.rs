//! Where speech files live, which model to fetch.

use std::path::{Path, PathBuf};

/// File name on phone.
pub(crate) const MODEL_FILE: &str = "ggml-small-q5_1.bin";
/// Pinned by commit; hash checked after download.
const MODEL_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small-q5_1.bin";
const MODEL_SHA256: &str = "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb";
const MODEL_BYTES: u64 = 190_085_487;

/// Engine libraries in package (Harbour: ELF `.so` only under `/usr/share/<name>/lib/`).
const BUNDLED_LIBS: &str = "/usr/share/harbour-piirit/lib/speech";

/// Model to fetch: URL, SHA-256 (hex), size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelSpec {
    /// Download address.
    pub url: String,
    /// Lowercase hex.
    pub sha256: String,
    /// Exact size.
    pub bytes: u64,
}

impl ModelSpec {
    /// Pinned model; tests swap all three via `PIIRIT_SPEECH_MODEL_URL`, `_SHA256`, `_BYTES`.
    pub(crate) fn current() -> Self {
        Self::from_env(|name| std::env::var(name).ok())
    }

    fn from_env(var: impl Fn(&str) -> Option<String>) -> Self {
        let pinned = Self {
            url: MODEL_URL.to_owned(),
            sha256: MODEL_SHA256.to_owned(),
            bytes: MODEL_BYTES,
        };
        // All three or none: half an override would fetch one file, check another.
        match (
            var("PIIRIT_SPEECH_MODEL_URL"),
            var("PIIRIT_SPEECH_MODEL_SHA256"),
            var("PIIRIT_SPEECH_MODEL_BYTES").and_then(|bytes| bytes.parse().ok()),
        ) {
            (Some(url), Some(sha256), Some(bytes)) => Self {
                url,
                sha256: sha256.to_ascii_lowercase(),
                bytes,
            },
            _ => pinned,
        }
    }
}

/// `speech/` beside accounts dir: `PIIRIT_ACCOUNTS_DIR`'s parent, else
/// `<XDG_DATA_HOME or ~/.local/share>/piirit/piirit/` (sailjail grant, as `accounts_dir`).
fn speech_dir_from(var: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    if let Some(accounts) = var("PIIRIT_ACCOUNTS_DIR").filter(|dir| !dir.is_empty()) {
        return Path::new(&accounts)
            .parent()
            .map(|parent| parent.join("speech"));
    }
    let base = var("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;
    Some(base.join("piirit/piirit/speech"))
}

/// Speech dir; not made.
pub(crate) fn speech_path() -> Option<PathBuf> {
    speech_dir_from(|name| std::env::var(name).ok())
}

/// Make speech dir, private, if missing.
pub(crate) fn make_speech_dir() -> Result<PathBuf, String> {
    let dir = speech_path().ok_or_else(|| "neither XDG_DATA_HOME nor HOME is set".to_owned())?;
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&dir)
        .map_err(|err| format!("cannot create the speech dir: {err}"))?;
    Ok(dir)
}

/// Model file.
pub(crate) fn model_path() -> Option<PathBuf> {
    speech_path().map(|dir| dir.join(MODEL_FILE))
}

/// Model while downloading.
pub(crate) fn partial_path(model: &Path) -> PathBuf {
    let mut name = model.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

/// Store file.
pub(crate) fn store_path() -> Option<PathBuf> {
    speech_path().map(|dir| dir.join("transcripts.sqlite"))
}

/// Engine libraries: `PIIRIT_SPEECH_LIBS`, package's, else
/// `speech/` beside binary or its parent (test binaries sit in `deps/`).
pub(crate) fn libs_dir() -> PathBuf {
    let exe = std::env::current_exe().ok();
    libs_dir_from(
        std::env::var("PIIRIT_SPEECH_LIBS").ok(),
        exe.as_deref(),
        Path::is_dir,
    )
}

fn libs_dir_from(
    run_time: Option<String>,
    exe: Option<&Path>,
    is_dir: impl Fn(&Path) -> bool,
) -> PathBuf {
    if let Some(dir) = run_time.filter(|dir| !dir.is_empty()) {
        return PathBuf::from(dir);
    }
    let bundled = PathBuf::from(BUNDLED_LIBS);
    if is_dir(&bundled) {
        return bundled;
    }
    exe.and_then(Path::parent)
        .into_iter()
        .flat_map(|dir| {
            [
                Some(dir.join("speech")),
                dir.parent().map(|up| up.join("speech")),
            ]
        })
        .flatten()
        .find(|dir| is_dir(dir))
        .unwrap_or(bundled)
}

#[cfg(test)]
mod tests {
    use super::{libs_dir_from, speech_dir_from, ModelSpec, MODEL_BYTES};
    use std::path::{Path, PathBuf};

    #[test]
    fn speech_files_sit_beside_the_accounts() {
        let vars = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| (*value).to_owned())
            }
        };
        assert_eq!(
            speech_dir_from(vars(&[
                ("PIIRIT_ACCOUNTS_DIR", "/t/x/accounts"),
                ("HOME", "/h")
            ])),
            Some(PathBuf::from("/t/x/speech"))
        );
        assert_eq!(
            speech_dir_from(vars(&[("XDG_DATA_HOME", "/d"), ("HOME", "/h")])),
            Some(PathBuf::from("/d/piirit/piirit/speech"))
        );
        assert_eq!(
            speech_dir_from(vars(&[("HOME", "/h")])),
            Some(PathBuf::from("/h/.local/share/piirit/piirit/speech"))
        );
        assert_eq!(speech_dir_from(vars(&[])), None);
    }

    #[test]
    fn engine_libraries_come_from_override_package_or_beside_the_binary() {
        let exe = Path::new("/b/target/debug/deps/test-1234");
        let none = |_: &Path| false;
        assert_eq!(
            libs_dir_from(Some("/run".into()), Some(exe), none),
            PathBuf::from("/run")
        );
        assert_eq!(
            libs_dir_from(None, Some(exe), |dir| dir
                == Path::new("/usr/share/harbour-piirit/lib/speech")),
            PathBuf::from("/usr/share/harbour-piirit/lib/speech")
        );
        assert_eq!(
            libs_dir_from(None, Some(exe), |dir| dir
                == Path::new("/b/target/debug/speech")),
            PathBuf::from("/b/target/debug/speech")
        );
        assert_eq!(
            libs_dir_from(None, Some(exe), none),
            PathBuf::from("/usr/share/harbour-piirit/lib/speech")
        );
    }

    #[test]
    fn model_override_takes_all_three_or_none() {
        let all = |name: &str| match name {
            "PIIRIT_SPEECH_MODEL_URL" => Some("http://127.0.0.1:1/m".to_owned()),
            "PIIRIT_SPEECH_MODEL_SHA256" => Some("AB".to_owned()),
            "PIIRIT_SPEECH_MODEL_BYTES" => Some("16".to_owned()),
            _ => None,
        };
        let spec = ModelSpec::from_env(all);
        assert_eq!(
            (spec.url.as_str(), spec.sha256.as_str(), spec.bytes),
            ("http://127.0.0.1:1/m", "ab", 16)
        );
        let half = |name: &str| all(name).filter(|_| name != "PIIRIT_SPEECH_MODEL_BYTES");
        assert_eq!(ModelSpec::from_env(half).bytes, MODEL_BYTES);
    }
}
