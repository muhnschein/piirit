//! About page: Piirit's and core's versions (core's asked once core is up), the versions copied
//! for a report, the intro slides opened as a tour (no way on into setup). Vs fake core.

// Qt harness: see qml_pages.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use piirit_shim::DeltaChatCore;
use qmetaobject::*;
use std::path::PathBuf;

mod common;

/// Probe imports components by absolute URL: data has no dir. Stack records last push with its
/// properties, keys in name order.
fn probe_qml() -> String {
    let components = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../qml/components");
    format!(
        r"
    import QtQuick 2.0
    import Sailfish.Silica 1.0
    import 'file://{}'
    Item {{
        Loader {{ id: loader }}
        QtObject {{
            id: stack
            property string pushed: ''
            property bool busy: false
            function push(url, props) {{
                var parts = []
                for (var key in props) {{ parts.push(key + '=' + props[key]) }}
                parts.sort()
                pushed = ('' + url).split('/').pop() + ':' + parts.join(',')
                return null
            }}
        }}
        function stackObject() {{ return stack }}
        function pushed() {{ return stack.pushed }}
        function clipboard() {{ return Clipboard.text }}
        function load(url) {{
            loader.setSource(url, {{ accountId: 1 }})
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
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
                return findIn(node.contentItem, name)
            }}
            return null
        }}
        function get(name, property) {{
            var item = findIn(loader.item, name)
            if (!item) {{ return 'missing:' + name }}
            return '' + item[property]
        }}
        function click(name) {{
            var item = findIn(loader.item, name)
            if (!item) {{ return 'missing:' + name }}
            item.clicked()
            return 'ok'
        }}
    }}
",
        components.display()
    )
}

#[test]
fn the_about_page_names_both_versions_and_copies_them_for_a_report() {
    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

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
                &[$(QVariant::from(QString::from($arg))),*],
            );
            QString::from_qvariant(result).unwrap_or_default()
        }};
    }

    let mut plan = common::Plan::default();

    // Opened before core is up: core's line waits for it, Piirit's does not.
    let s = steps.clone();
    plan.after(1, move || unsafe {
        common::record(&s, "load", call!("load", common::page_url("AboutPage.qml")));
        common::record(&s, "app-early", call!("get", "appVersion", "text"));
        common::record(&s, "core-early", call!("get", "coreVersion", "text"));
    });
    let core_ptr: *const QObjectBox<DeltaChatCore> = std::ptr::addr_of!(core_box);
    plan.after(1, move || unsafe {
        (*core_ptr)
            .pinned()
            .borrow_mut()
            .start(QString::from(env!("CARGO_BIN_EXE_fake-core-server")));
    });
    let s = steps.clone();
    plan.until(1, 50, move |last| unsafe {
        let line = call!("get", "coreVersion", "text");
        if line.to_string().is_empty() && !last {
            return false;
        }
        common::record(&s, "core-line", line);
        common::record(&s, "report", call!("click", "reportEntry"));
        common::record(&s, "copied", call!("clipboard"));
        common::record(&s, "intro", call!("click", "introEntry"));
        common::record(&s, "intro-pushed", call!("pushed"));
        true
    });
    plan.after(1, move || unsafe {
        (*engine_ptr).quit();
    });

    plan.run();
    engine.exec();

    let steps = steps.borrow();
    let value = |label: &str| common::value_of(&steps, label).to_string();
    let context = format!("steps: {steps:?}");

    let app_line = format!("Piirit {}", env!("CARGO_PKG_VERSION"));
    assert_eq!(
        value("load"),
        "ok",
        "the About page did not load. {context}"
    );
    assert_eq!(
        (value("app-early"), value("core-early")),
        (app_line.clone(), String::new()),
        "Piirit's version is not there at once, or core's is guessed before core says. {context}"
    );
    // Fake core says what the pinned real one says (`real_server.rs` holds that to the pin).
    assert_eq!(
        value("core-line"),
        "Delta Chat core 2.68.0",
        "core's version was not asked once core came up, or not shown without its v. {context}"
    );
    assert_eq!(
        (value("report"), value("copied")),
        (
            "ok".to_string(),
            format!("{app_line}\nDelta Chat core 2.68.0")
        ),
        "Report a problem did not copy both versions. {context}"
    );
    assert_eq!(
        value("intro-pushed"),
        "IntroPage.qml:tour=true",
        "About Delta Chat did not open the intro slides as a tour. {context}"
    );
}
