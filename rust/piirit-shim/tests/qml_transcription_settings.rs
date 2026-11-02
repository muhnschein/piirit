//! Turning transcription on: Settings switch asks the experimental question, then languages
//! (phone's first and picked; greyed ones can't be picked; Next needs one), which turn the
//! feature on, then when to transcribe; Done returns to Settings. Cancel on languages leaves it
//! off. Once on, an entry under the switch names languages and timing and opens the
//! transcription page: storage, transcripts, delete all after the platform's countdown.
//!
//! Real pages vs stub Silica; page stack here builds pages it is asked for, as Silica does.
//! Phone language Finnish (`LC_ALL`).

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use std::path::PathBuf;

mod common;

/// Probe imports components by absolute URL: data has no dir.
#[allow(clippy::too_many_lines)]
fn probe_qml() -> String {
    let components = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    format!(
        r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import 'file://{}'
    import 'file://{}/../js/Speech.js' as Speech
    Item {{
        width: 540
        height: 960
        property string calls: ''

        Loader {{ id: loader; width: 540; height: 960 }}
        Item {{ id: holder; width: 540; height: 960 }}

        // App's Transcriber, as far as these pages read it.
        QtObject {{
            id: transcriber
            property bool enabled: Settings.transcriptionEnabled === true
            property int model_state: 1
            property int model_permille: 340
            property string model_name: 'Whisper small'
            property real model_download_bytes: 190085487
            property real model_bytes: 0
            property int transcript_count: 0
            property real transcript_bytes: 0
            function delete_all() {{ calls += 'delete_all;' }}
            function download_model() {{ calls += 'download_model;' }}
        }}

        // Silica's page stack: builds what is pushed, in order; pop to a page drops those above.
        QtObject {{
            id: stack
            property var pages: []
            property string log: ''
            property bool busy: false
            function nameOf(url) {{ return ('' + url).split('/').pop() }}
            function push(target, props) {{
                if (typeof target === 'object' && target !== null && !('' + target).match(/^file:/)) {{
                    while (pages.length > 0 && pages[pages.length - 1] !== target) {{
                        pages.pop().destroy()
                    }}
                    log += 'pop;'
                    return target
                }}
                var made = Qt.createComponent(target)
                if (made.status !== Component.Ready) {{
                    log += 'broken:' + nameOf(target) + ':' + made.errorString() + ';'
                    return null
                }}
                var page = made.createObject(holder, props || {{}})
                pages.push(page)
                log += 'push:' + nameOf(target) + ';'
                return page
            }}
            function replace(target, props) {{
                var gone = pages.pop()
                var page = push(target, props)
                gone.destroy()
                log = log.replace(/push:([^;]*);$/, 'replace:$1;')
                return page
            }}
            function pop() {{ pages.pop().destroy(); log += 'pop;' }}
        }}
        function stackObject() {{ return stack }}
        function top() {{ return stack.pages.length > 0 ? stack.pages[stack.pages.length - 1] : null }}
        function log() {{ var was = stack.log; stack.log = ''; return was }}
        function depth() {{ return '' + stack.pages.length }}

        function load(url, props) {{
            loader.setSource(url, JSON.parse(props))
            if (loader.status !== Loader.Ready) {{ return 'load-failed' }}
            if (loader.item.transcriber !== undefined) {{ loader.item.transcriber = transcriber }}
            return 'ok'
        }}
        function findIn(node, name) {{
            if (!node) {{ return null }}
            if (node.objectName === name) {{ return node }}
            var kids = node.data !== undefined ? node.data : node.children
            for (var i = 0; kids && i < kids.length; i++) {{
                var hit = findIn(kids[i], name)
                if (hit) {{ return hit }}
            }}
            if (node.contentItem && node.contentItem !== node) {{
                var inside = findIn(node.contentItem, name)
                if (inside) {{ return inside }}
            }}
            if (node.menu) {{
                var inMenu = findIn(node.menu, name)
                if (inMenu) {{ return inMenu }}
            }}
            return null
        }}
        function at(where) {{ return where === 'top' ? top() : loader.item }}
        function get(where, name, property) {{
            var item = findIn(at(where), name)
            if (!item) {{ return 'missing:' + name }}
            return '' + item[property]
        }}
        function click(where, name) {{
            var item = findIn(at(where), name)
            if (!item) {{ return 'missing:' + name }}
            item.clicked()
            return 'ok'
        }}
        function topSays(property) {{ return '' + top()[property] }}
        function accept() {{ top().accept(); return 'ok' }}
        function pop() {{ stack.pop(); return 'ok' }}
        function reject() {{ var page = stack.pages.pop(); page.reject(); page.destroy(); return 'ok' }}
        // Language rows in drawn order: code, picked, enabled.
        function languages() {{
            var found = []
            var seen = []
            var walk = function (node) {{
                if (!node || seen.indexOf(node) >= 0) {{ return }}
                seen.push(node)
                if (('' + node.objectName).indexOf('language_') === 0) {{
                    found.push(node.objectName.substring(9) + (node.checked ? '+' : '')
                               + (node.enabled ? '' : '!'))
                }}
                var kids = node.children
                for (var i = 0; kids && i < kids.length; i++) {{ walk(kids[i]) }}
                if (node.contentItem && node.contentItem !== node) {{ walk(node.contentItem) }}
            }}
            walk(top())
            return found.join(',')
        }}
        function appReads(name) {{ return '' + Settings[name] }}
        function appWrites(name, value) {{ Settings[name] = value; return 'ok' }}
        function remorse() {{
            var popup = findIn(loader.item, 'deleteRemorse')
            return popup.active + ':' + popup.text
        }}
        function heard() {{ var was = calls; calls = ''; return was }}
        function codes() {{
            return ['fi_FI', 'nb_NO', 'zh_HK', 'zh_CN', 'C'].map(Speech.codeOf).join(',')
        }}
        function storeSome() {{
            transcriber.model_state = 2
            transcriber.model_bytes = 190085487
            transcriber.transcript_count = 132
            transcriber.transcript_bytes = 48000
            return 'ok'
        }}
    }}
",
        components.display(),
        components.display()
    )
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_switch_leads_through_languages_and_timing_and_cancel_leaves_it_off() {
    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
        std::env::set_var("LC_ALL", "de_DE.UTF-8");
        std::env::set_var("LANG", "de_DE.UTF-8");
    }

    // Settings page's deletion count asks core; unstarted, it answers nothing.
    let core_box = QObjectBox::new(DeltaChatCore::default());
    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    engine.set_object_property("core".into(), core_box.pinned());
    engine.load_data(QByteArray::from(probe_qml()));
    let stack = engine.invoke_method("stackObject".into(), &[]);
    engine.set_property("pageStack".into(), stack);

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let steps: common::Steps = common::Steps::default();

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result).unwrap_or_default()
        }};
    }
    macro_rules! q {
        ($text:expr) => {
            QString::from($text)
        };
    }

    let mut plan = common::Plan::default();

    let s = steps.clone();
    plan.after(1, move || unsafe {
        let url = q!(common::page_url("SettingsPage.qml"));
        common::record(&s, "load", call!("load", url, q!("{}")));
        common::record(&s, "codes", call!("codes"));
        common::record(
            &s,
            "off-switch",
            call!("get", q!("page"), q!("transcriptionSwitch"), q!("checked")),
        );
        common::record(
            &s,
            "off-entry",
            call!("get", q!("page"), q!("transcriptionEntry"), q!("visible")),
        );

        // First try: question, languages, cancel.
        common::record(
            &s,
            "flip",
            call!("click", q!("page"), q!("transcriptionSwitch")),
        );
        common::record(&s, "asked", call!("log"));
        common::record(&s, "asked-title", call!("topSays", q!("title")));
        common::record(&s, "accept-question", call!("accept"));
        common::record(&s, "to-languages", call!("log"));
        common::record(&s, "languages-setup", call!("topSays", q!("setup")));
        common::record(&s, "languages", call!("languages"));
        common::record(&s, "can-next", call!("topSays", q!("canAccept")));
        common::record(
            &s,
            "on-before-next",
            call!("appReads", q!("transcriptionEnabled")),
        );
        common::record(&s, "cancel", call!("reject"));
        common::record(
            &s,
            "on-after-cancel",
            call!("appReads", q!("transcriptionEnabled")),
        );
        common::record(
            &s,
            "switch-after-cancel",
            call!("get", q!("page"), q!("transcriptionSwitch"), q!("checked")),
        );
        common::record(&s, "depth-after-cancel", call!("depth"));

        // Second try: through to the end.
        call!("click", q!("page"), q!("transcriptionSwitch"));
        call!("accept");
        call!("log");
        // Greyed: no pick. Last one off: no Next.
        common::record(
            &s,
            "pick-greyed",
            call!("click", q!("top"), q!("language_lt")),
        );
        common::record(
            &s,
            "unpick-phone",
            call!("click", q!("top"), q!("language_de")),
        );
        common::record(&s, "no-language", call!("languages"));
        common::record(&s, "can-next-none", call!("topSays", q!("canAccept")));
        call!("click", q!("top"), q!("language_de"));
        call!("click", q!("top"), q!("language_en"));
        common::record(&s, "accept-languages", call!("accept"));
        common::record(&s, "to-when", call!("log"));
        common::record(
            &s,
            "on-after-next",
            call!("appReads", q!("transcriptionEnabled")),
        );
        common::record(
            &s,
            "languages-stored",
            call!("appReads", q!("transcriptionLanguages")),
        );
        common::record(
            &s,
            "when-progress",
            call!("get", q!("top"), q!("modelProgress"), q!("text")),
        );
        common::record(
            &s,
            "when-mode",
            call!(
                "get",
                q!("top"),
                q!("transcriptionModeCombo"),
                q!("currentIndex")
            ),
        );
        common::record(
            &s,
            "when-mode-says",
            call!(
                "get",
                q!("top"),
                q!("transcriptionModeCombo"),
                q!("description")
            ),
        );
        common::record(
            &s,
            "pick-charging",
            call!("click", q!("top"), q!("transcriptionMode1")),
        );
        common::record(
            &s,
            "mode-stored",
            call!("appReads", q!("transcriptionMode")),
        );
        common::record(
            &s,
            "when-mode-now",
            call!(
                "get",
                q!("top"),
                q!("transcriptionModeCombo"),
                q!("description")
            ),
        );
        common::record(
            &s,
            "pick-older",
            call!("click", q!("top"), q!("transcriptionOlderSwitch")),
        );
        common::record(
            &s,
            "older-stored",
            call!("appReads", q!("transcriptionOlder")),
        );
        common::record(&s, "done", call!("accept"));
        common::record(&s, "back-to-settings", call!("log"));
        common::record(&s, "depth-after-done", call!("depth"));

        // Settings now: switch on, entry names languages and timing.
        common::record(
            &s,
            "on-switch",
            call!("get", q!("page"), q!("transcriptionSwitch"), q!("checked")),
        );
        common::record(
            &s,
            "on-entry",
            call!("get", q!("page"), q!("transcriptionEntry"), q!("visible")),
        );
        common::record(
            &s,
            "entry-says",
            call!("get", q!("page"), q!("transcriptionDetail"), q!("text")),
        );
        common::record(
            &s,
            "open-entry",
            call!("click", q!("page"), q!("transcriptionEntry")),
        );
        common::record(&s, "to-page", call!("log"));

        // Off: at once, nothing asked; languages kept for next time.
        call!("pop");
        call!("log");
        common::record(
            &s,
            "flip-off",
            call!("click", q!("page"), q!("transcriptionSwitch")),
        );
        common::record(&s, "off-asked", call!("log"));
        common::record(
            &s,
            "off-again",
            call!("appReads", q!("transcriptionEnabled")),
        );
        common::record(
            &s,
            "off-languages",
            call!("appReads", q!("transcriptionLanguages")),
        );
        common::record(
            &s,
            "off-entry-again",
            call!("get", q!("page"), q!("transcriptionEntry"), q!("visible")),
        );
    });

    // Transcription page alone.
    let s = steps.clone();
    plan.after(1, move || unsafe {
        call!("appWrites", q!("transcriptionEnabled"), true);
        let url = q!(common::page_url("TranscriptionPage.qml"));
        common::record(&s, "page-load", call!("load", url, q!("{}")));
        common::record(
            &s,
            "page-languages",
            call!("get", q!("page"), q!("languagesEntry"), q!("value")),
        );
        common::record(
            &s,
            "page-model-loading",
            call!("get", q!("page"), q!("modelLabel"), q!("text")),
        );
        call!("appWrites", q!("transcriptionMode"), 2);
        common::record(
            &s,
            "page-mode",
            call!(
                "get",
                q!("page"),
                q!("transcriptionModeCombo"),
                q!("currentIndex")
            ),
        );
        common::record(
            &s,
            "page-mode-says",
            call!(
                "get",
                q!("page"),
                q!("transcriptionModeCombo"),
                q!("description")
            ),
        );
        common::record(
            &s,
            "page-none-delete",
            call!("get", q!("page"), q!("deleteAllButton"), q!("enabled")),
        );
        common::record(
            &s,
            "page-open-languages",
            call!("click", q!("page"), q!("languagesEntry")),
        );
        common::record(&s, "page-languages-opened", call!("log"));
        common::record(&s, "page-languages-setup", call!("topSays", q!("setup")));
        common::record(&s, "page-languages-kept", call!("languages"));
        call!("reject");
    });

    let s = steps.clone();
    plan.after(1, move || unsafe {
        // Model on phone, transcripts stored.
        call!("storeSome");
        common::record(
            &s,
            "page-model",
            call!("get", q!("page"), q!("modelLabel"), q!("text")),
        );
        common::record(
            &s,
            "page-transcripts",
            call!("get", q!("page"), q!("transcriptsLabel"), q!("text")),
        );
        common::record(
            &s,
            "page-delete",
            call!("click", q!("page"), q!("deleteAllButton")),
        );
        common::record(&s, "page-countdown", call!("remorse"));
        common::record(&s, "page-not-yet", call!("heard"));
    });

    let s = steps.clone();
    plan.until(1, 5, move |last| unsafe {
        let heard = call!("heard").to_string();
        if heard.is_empty() && !last {
            return false;
        }
        common::record(&s, "page-deleted", heard.into());
        (*engine_ptr).quit();
        true
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow();
    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");

    assert_eq!(
        value("load"),
        "ok",
        "the settings page did not load. {context}"
    );
    assert_eq!(
        value("codes"),
        "fi,no,yue,zh,en",
        "a phone language does not map to its whisper code. {context}"
    );
    assert_eq!(
        (value("off-switch"), value("off-entry")),
        ("false".to_string(), "false".to_string()),
        "a fresh phone has transcription on, or shows its entry. {context}"
    );
    assert_eq!(
        (value("asked"), value("asked-title")),
        (
            "push:ExperimentalFeatureDialog.qml;".to_string(),
            "Transcribe voice messages (experimental)".to_string()
        ),
        "turning transcription on did not first ask the experimental question. {context}"
    );
    assert_eq!(
        (value("to-languages"), value("languages-setup")),
        (
            "replace:TranscriptionLanguagesDialog.qml;".to_string(),
            "true".to_string()
        ),
        "accepting the question did not lead on to the languages. {context}"
    );
    let languages = value("languages");
    assert!(
        languages.starts_with("de+,en,es,fr,"),
        "the phone's language does not come first, picked, with the rest by name: \
         {languages}. {context}"
    );
    assert!(
        languages.ends_with(
            ",zh,bn!,bg!,cs!,da!,et!,el!,gu!,hi!,kn!,lv!,lt!,hu!,ml!,mr!,no!,pa!,ro!,sk!,sl!,fi!,sv!,\
             ta!,tt!,te!,yue!"
        ),
        "the languages not available yet are not listed greyed, after the rest: \
         {languages}. {context}"
    );
    assert_eq!(
        languages.matches(',').count(),
        37,
        "not 38 languages: {languages}"
    );
    assert_eq!(
        value("can-next"),
        "true",
        "Next is off with a language picked. {context}"
    );
    assert_eq!(
        value("on-before-next"),
        "false",
        "transcription went on before the languages were accepted. {context}"
    );
    assert_eq!(
        (
            value("on-after-cancel"),
            value("switch-after-cancel"),
            value("depth-after-cancel")
        ),
        ("false".to_string(), "false".to_string(), "0".to_string()),
        "cancelling the languages did not leave transcription off. {context}"
    );
    assert_eq!(
        value("no-language"),
        languages.replacen("de+", "de", 1).as_str(),
        "a greyed language could be picked, or the last one not taken off. {context}"
    );
    assert_eq!(
        value("can-next-none"),
        "false",
        "Next is on with no language picked. {context}"
    );
    assert_eq!(
        (
            value("to-when"),
            value("on-after-next"),
            value("languages-stored")
        ),
        (
            "push:TranscriptionWhenDialog.qml;".to_string(),
            "true".to_string(),
            "de,en".to_string()
        ),
        "Next did not store the languages, turn transcription on and ask when. {context}"
    );
    assert_eq!(
        value("when-progress"),
        "Downloading the speech model… 34%",
        "the timing dialog does not show the download it started. {context}"
    );
    assert_eq!(
        (value("when-mode"), value("when-mode-says")),
        (
            "0".to_string(),
            "Tap Transcribe on a voice message to turn it into text.".to_string()
        ),
        "a fresh phone does not start on Never, said in one sentence. {context}"
    );
    assert_eq!(
        (value("mode-stored"), value("when-mode-now")),
        (
            "1".to_string(),
            "New voice messages are transcribed while the phone is charging.".to_string()
        ),
        "While charging was not stored, or the line under it did not follow. {context}"
    );
    assert_eq!(
        value("older-stored"),
        "true",
        "the older switch stored nothing. {context}"
    );
    assert_eq!(
        (value("back-to-settings"), value("depth-after-done")),
        ("pop;".to_string(), "0".to_string()),
        "Done did not return to Settings. {context}"
    );
    assert_eq!(
        (value("on-switch"), value("on-entry")),
        ("true".to_string(), "true".to_string()),
        "after setup the switch is off, or its entry missing. {context}"
    );
    assert_eq!(
        value("entry-says"),
        "Deutsch, English · Auto-transcribe: While charging",
        "the entry does not name languages and timing. {context}"
    );
    assert_eq!(
        value("to-page"),
        "push:TranscriptionPage.qml;",
        "the entry did not open the transcription page. {context}"
    );
    assert_eq!(
        (
            value("off-asked"),
            value("off-again"),
            value("off-languages"),
            value("off-entry-again")
        ),
        (
            String::new(),
            "false".to_string(),
            "de,en".to_string(),
            "false".to_string()
        ),
        "turning off asked something, did not turn off, or lost the languages. {context}"
    );

    assert_eq!(
        value("page-load"),
        "ok",
        "the transcription page did not load. {context}"
    );
    assert_eq!(
        value("page-languages"),
        "Deutsch, English",
        "the transcription page does not name the languages. {context}"
    );
    assert_eq!(
        value("page-model-loading"),
        "Whisper small · Downloading the speech model… 34%",
        "the page does not show the download. {context}"
    );
    assert_eq!(
        (value("page-mode"), value("page-mode-says")),
        (
            "2".to_string(),
            "New voice messages are transcribed once they have downloaded. This uses battery."
                .to_string()
        ),
        "the page's timing does not follow the setting. {context}"
    );
    assert_eq!(
        value("page-none-delete"),
        "false",
        "Delete all transcripts is offered with none stored. {context}"
    );
    assert_eq!(
        (
            value("page-languages-opened"),
            value("page-languages-setup"),
            value("page-languages-kept")
        ),
        (
            "push:TranscriptionLanguagesDialog.qml;".to_string(),
            "false".to_string(),
            languages.replacen("de+,en,", "de+,en+,", 1)
        ),
        "Languages did not reopen the list with the stored picks. {context}"
    );
    assert_eq!(
        (value("page-model"), value("page-transcripts")),
        (
            "Whisper small · 190.1 MB on this phone".to_string(),
            "132 voice message(s) · 48.0 kB".to_string()
        ),
        "the page does not say what transcription stores. {context}"
    );
    assert_eq!(
        (value("page-countdown"), value("page-not-yet")),
        ("true:Deleting all transcripts".to_string(), String::new()),
        "Delete all transcripts did not wait on the platform's countdown first. {context}"
    );
    assert_eq!(
        value("page-deleted"),
        "delete_all;",
        "the countdown ran out and nothing was deleted. {context}"
    );
}
