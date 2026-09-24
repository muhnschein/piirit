//! Every language catalog carries every string, translated.
//!
//! A scan of the `.ts` files rather than anything Qt: `lupdate` keeps the
//! catalogs in step with the source and `ci/packaging-lint.sh` proves
//! that, but neither says whether a string in one of them has been
//! translated. A string left `unfinished` is one the reader of that
//! language meets in English, and with forty catalogs nobody notices
//! from the diff.
//!
//! `piirit.ts` is the untranslated source catalog, and `piirit-en.ts`
//! exists only for its plural forms, so those two are held to different
//! rules.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn translations_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../translations")
}

/// The `(context, source)` pairs a catalog carries, and how many of
/// them are still unfinished.
fn read_catalog(text: &str) -> (BTreeSet<(String, String)>, Vec<String>) {
    let mut context = String::new();
    let mut sources = BTreeSet::new();
    let mut unfinished = Vec::new();
    let mut source = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line
            .strip_prefix("<name>")
            .and_then(|rest| rest.strip_suffix("</name>"))
        {
            context = name.to_string();
        } else if let Some(text) = line
            .strip_prefix("<source>")
            .and_then(|rest| rest.strip_suffix("</source>"))
        {
            source = text.to_string();
            sources.insert((context.clone(), source.clone()));
        } else if line.starts_with("<translation type=\"unfinished\"") {
            unfinished.push(format!("{context}: {source}"));
        }
    }
    (sources, unfinished)
}

#[test]
fn every_language_catalog_is_complete() {
    let dir = translations_dir();
    let (wanted, _) =
        read_catalog(&fs::read_to_string(dir.join("piirit.ts")).expect("read the source catalog"));
    assert!(
        wanted.len() > 100,
        "the source catalog holds only {} strings; did lupdate stop running?",
        wanted.len()
    );

    let mut catalogs: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("read translations/")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            name.is_some_and(|n| n.starts_with("piirit-"))
                && path.extension().is_some_and(|ext| ext == "ts")
        })
        .collect();
    catalogs.sort();
    assert!(
        catalogs.len() >= 40,
        "only {} language catalogs; Sailfish ships in more languages than that",
        catalogs.len()
    );

    let mut problems = Vec::new();
    for path in &catalogs {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = fs::read_to_string(path).unwrap_or_else(|err| panic!("read {name}: {err}"));
        // The language attribute is what lupdate counts the plural forms
        // from, and what says which reader the file is for.
        let language = name.trim_start_matches("piirit-").trim_end_matches(".ts");
        if !text.contains(&format!("<TS version=\"2.1\" language=\"{language}\"")) {
            problems.push(format!("{name}: its language attribute is not {language}"));
        }
        let (sources, unfinished) = read_catalog(&text);
        if sources != wanted {
            let missing: Vec<_> = wanted.difference(&sources).take(3).collect();
            let extra: Vec<_> = sources.difference(&wanted).take(3).collect();
            problems.push(format!(
                "{name}: not the source catalog's strings (missing {missing:?}, extra {extra:?}); \
                 run scripts/update-translations.sh"
            ));
        }
        // English needs only its plurals: everything else falls back to
        // the source text, which is English already.
        if language == "en" {
            let plurals_untranslated: Vec<_> = unfinished
                .iter()
                .filter(|entry| entry.contains("%n"))
                .collect();
            if !plurals_untranslated.is_empty() {
                problems.push(format!(
                    "{name}: plural forms untranslated: {plurals_untranslated:?}"
                ));
            }
        } else if !unfinished.is_empty() {
            problems.push(format!(
                "{name}: {} untranslated, starting with {:?}",
                unfinished.len(),
                &unfinished[..unfinished.len().min(3)]
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "these catalogs would show a reader English where their language was promised:\n  {}",
        problems.join("\n  ")
    );
}
