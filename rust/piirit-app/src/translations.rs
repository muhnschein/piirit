//! Load catalog for reader's language.
//!
//! This crate's `unsafe` and C++ build step: qmetaobject doesn't bind `QTranslator`. Kept to one
//! short block.
//!
//! Silica strings come translated with system; these are app's: `translations/piirit-<lang>.qm`
//! (RPM runs lrelease on `.ts`). Follows reader's Language via startup locale: `de_DE`, then
//! `de`; no catalog: nothing loaded, strings as written.

// `cpp!` call crosses FFI, so unsafe. Only this file; see docs/PROJECT.md.
#![allow(unsafe_code)]

use cpp::cpp;
use qmetaobject::QString;

cpp! {{
    #include <QtCore/QCoreApplication>
    #include <QtCore/QLocale>
    #include <QtCore/QString>
    #include <QtCore/QTranslator>
}}

/// Catalog file name prefix.
const CATALOG: &str = "piirit";

/// Install catalog for `locale` from `dir`; true if found.
///
/// Empty `locale`: system's (reader's Settings choice). Else e.g. `de_DE`, for tests.
///
/// After app object (after view, in `main`), before QML: Qt 5.6 cannot retranslate built pages.
pub fn install(dir: &str, locale: &str) -> bool {
    let dir = QString::from(dir);
    let locale = QString::from(locale);
    let name = QString::from(CATALOG);
    // SAFETY: QStrings owned by this frame, read by value. Translator parented to app object,
    // which owns it; no Rust pointer kept.
    cpp!(unsafe [dir as "QString", locale as "QString", name as "QString"] -> bool as "bool" {
        QCoreApplication *app = QCoreApplication::instance();
        if (!app) {
            return false;
        }
        QTranslator *translator = new QTranslator(app);
        QLocale which = locale.isEmpty() ? QLocale() : QLocale(locale);
        // "-" between name and locale. Most specific first: pt_BR beats pt.
        if (!translator->load(which, name, QStringLiteral("-"), dir)) {
            delete translator;
            return false;
        }
        return app->installTranslator(translator);
    })
}

// Qt harness, as shim tests: `unused_unsafe` (`set_var` unsafe only from edition 2024),
// `borrow_as_ptr` (engine pointer), `disallowed_methods` (`single_shot` Duration), `expect_used`.
#[cfg(test)]
#[allow(
    unused_unsafe,
    clippy::borrow_as_ptr,
    clippy::disallowed_methods,
    clippy::expect_used
)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use qmetaobject::*;

    use super::install;

    /// Catalogs in source tree.
    fn translations_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../translations")
    }

    /// Compile catalog into `out` via `lrelease`. None if absent: local skip, CI failure.
    fn compile(language: &str, out: &std::path::Path) -> Option<()> {
        let lrelease = ["lrelease", "lrelease-qt5"].into_iter().find(|tool| {
            std::process::Command::new(tool)
                .arg("-version")
                .output()
                .is_ok()
        });
        let Some(lrelease) = lrelease else {
            assert!(
                std::env::var_os("CI").is_none(),
                "lrelease is not installed, so the catalogs cannot be compiled; \
                 install qttools5-dev-tools"
            );
            eprintln!("skipping: lrelease not found (install qttools5-dev-tools)");
            return None;
        };
        let source = translations_dir().join(format!("piirit-{language}.ts"));
        let target = out.join(format!("piirit-{language}.qm"));
        let status = std::process::Command::new(lrelease)
            .arg(&source)
            .arg("-qm")
            .arg(&target)
            .status()
            .expect("run lrelease");
        assert!(status.success(), "lrelease failed on {}", source.display());
        Some(())
    }

    // One app object per process; translators apply to later loads.
    #[test]
    fn the_readers_language_is_loaded_and_its_plurals_counted() {
        let out = std::env::temp_dir().join(format!("piirit-translations-{}", std::process::id()));
        std::fs::create_dir_all(&out).expect("create the output dir");
        if compile("de", &out).is_none() {
            return;
        }

        // SAFETY: single-threaded; set before Qt starts.
        unsafe {
            std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        }
        // App object: translator needs it.
        let mut engine = QmlEngine::new();
        let dir = out.to_string_lossy().into_owned();

        assert!(
            !install(&dir, "xx_YY"),
            "a language with no catalog reports one loaded, so a reader of \
             it would get the wrong language rather than the source text"
        );
        assert!(
            install(&dir, "de_DE"),
            "the German catalog did not load from {dir}"
        );

        // Context as qsTr() in that page. Core words: via window's table.
        // Script qsTr() context = file name, must match lupdate.
        let stock = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/js/StockStrings.js");
        engine.load_data(QByteArray::from(
            r"
            import QtQuick 2.0
            import '__STOCK__' as Stock
            Item {
                function stock() { return Stock.all()[3] + '|' + Stock.all()[129] }
                function plain() { return qsTranslate('WelcomePage', 'Set up your profile') }
                function one() { return qsTranslate('GroupPage', '%n member(s)', '', 1) }
                function many() { return qsTranslate('GroupPage', '%n member(s)', '', 3) }
                function untranslated() { return qsTranslate('Nowhere', 'not in any catalog') }
            }
            "
            .replace("__STOCK__", &format!("file://{}", stock.display()))
            .as_str(),
        ));
        let engine_ptr = std::ptr::addr_of_mut!(engine);
        let mut seen: Vec<(&str, String)> = Vec::new();
        let seen_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(seen);
        single_shot(Duration::from_millis(200), move || unsafe {
            for name in ["plain", "one", "many", "untranslated", "stock"] {
                let value = (*engine_ptr).invoke_method(name.into(), &[]);
                let text = QString::from_qvariant(value)
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                (*seen_ptr).push((name, text));
            }
            (*engine_ptr).quit();
        });
        engine.exec();

        let value = |label: &str| {
            seen.iter()
                .find(|(name, _)| *name == label)
                .map(|(_, value)| value.clone())
                .unwrap_or_default()
        };
        assert_eq!(value("plain"), "Profil einrichten", "seen: {seen:?}");
        assert_eq!(value("one"), "1 Mitglied", "seen: {seen:?}");
        assert_eq!(value("many"), "3 Mitglieder", "seen: {seen:?}");
        assert_eq!(
            value("stock"),
            "Entwurf|%1$s hinzugefügt von %2$s.",
            "the core's words are not translated, so it writes English into a \
             German reader's chats. seen: {seen:?}"
        );
        assert_eq!(
            value("untranslated"),
            "not in any catalog",
            "a string with no translation does not fall back to itself. seen: {seen:?}"
        );
        let _ = std::fs::remove_dir_all(&out);
    }
}
