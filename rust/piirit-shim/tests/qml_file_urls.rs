//! Peer file name -> URL via per-segment encoding. `encodeURI` leaves `#`, `?` alone
//! (`a#b.png` pointed at `a`); use `encodeURIComponent` per segment. Measured on
//! the two components that build it. Avatars: image provider `src/pictures.rs`,
//! one encoded value; measure: picture under such name found and drawn.

// Qt harness: see qml_avatar.rs.
#![allow(unsafe_code, unused_unsafe, clippy::borrow_as_ptr, clippy::expect_used)]

use qmetaobject::*;

mod common;

const PROBE_QML: &str = r"
    import QtQuick 2.0
    Item {
        Loader { id: loader }
        function load(url, property, value) {
            loader.setSource('', {})
            var initial = { width: 540 }
            initial[property] = value
            loader.setSource(url, initial)
            return loader.status === Loader.Ready ? 'ok' : 'load-failed'
        }
        function findIn(node, name) {
            if (!node) { return null }
            if (node.objectName === name) { return node }
            var kids = node.children
            for (var i = 0; kids && i < kids.length; i++) {
                var hit = findIn(kids[i], name)
                if (hit) { return hit }
            }
            return null
        }
        function get(name, property) {
            var item = findIn(loader.item, name)
            if (!item) { return 'missing:' + name }
            return '' + item[property]
        }
        function rootProperty(property) { return '' + loader.item[property] }
    }
";

/// Chars URL reads as syntax: percent, space, fragment, query.
const AWKWARD: &str = "/home/u/.local/share/piirit/blobs/100% sure#1?.png";

#[test]
#[allow(clippy::too_many_lines)]
fn a_file_url_encodes_every_character_that_is_not_a_slash() {
    // SAFETY: single thread; pre-Qt.
    unsafe {
        std::env::set_var("QT_QPA_PLATFORM", "offscreen");
    }

    // Real picture under awkward name, which also holds recipe's own `&` and `=`.
    let blobs = std::env::temp_dir().join(format!("piirit-file-urls-{}", std::process::id()));
    std::fs::create_dir_all(&blobs).expect("a temporary directory");
    let awkward_picture = blobs.join("100% sure#1?&x=y.png");
    std::fs::copy(common::a_real_picture(), &awkward_picture).expect("the picture copies");
    let awkward_picture = awkward_picture.display().to_string();

    let mut engine = QmlEngine::new();
    engine.add_import_path(QString::from(
        common::stubs_dir().to_string_lossy().into_owned(),
    ));
    piirit_shim::install_pictures(&engine);
    engine.load_data(QByteArray::from(PROBE_QML));

    let engine_ptr = std::ptr::addr_of_mut!(engine);
    let mut steps: Vec<(&str, String)> = Vec::new();
    let steps_ptr: *mut Vec<(&str, String)> = std::ptr::addr_of_mut!(steps);

    macro_rules! call {
        ($name:expr $(, $arg:expr)*) => {{
            let result = (*engine_ptr).invoke_method(
                $name.into(),
                &[$(QVariant::from($arg)),*],
            );
            QString::from_qvariant(result)
                .map(|value| value.to_string())
                .unwrap_or_default()
        }};
    }
    macro_rules! record {
        ($label:expr, $value:expr) => {
            (*steps_ptr).push(($label, $value))
        };
    }

    let mut plan = common::Plan::default();

    plan.after(1, move || unsafe {
        record!(
            "attachment-load",
            call!(
                "load",
                QString::from(common::component_url("AttachmentPreview.qml")),
                QString::from("filePath"),
                QString::from(AWKWARD)
            )
        );
        record!(
            "attachment-url",
            call!("rootProperty", QString::from("fileUrl"))
        );
    });

    plan.after(1, move || unsafe {
        record!(
            "avatar-load",
            call!(
                "load",
                QString::from(common::component_url("Avatar.qml")),
                QString::from("picturePath"),
                QString::from(awkward_picture.as_str())
            )
        );
        record!(
            "avatar-url",
            call!("get", QString::from("avatarImage"), QString::from("source"))
        );
    });

    // Loaded off main thread: read once in, however slow the load (`Image.Ready` is 1).
    plan.until(1, 30, move |last| unsafe {
        let status = call!("get", QString::from("avatarImage"), QString::from("status"));
        if status != "1" && !last {
            return false;
        }
        record!("avatar-status", status);
        (*engine_ptr).quit();
        true
    });

    plan.run();
    engine.exec();

    let value = |label: &str| {
        steps
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value.clone())
            .unwrap_or_default()
    };
    let context = format!("steps: {steps:?}");

    let _ = std::fs::remove_dir_all(&blobs);

    assert_eq!(
        value("attachment-load"),
        "ok",
        "AttachmentPreview did not load. {context}"
    );
    let url = value("attachment-url");
    let path = url
        .strip_prefix("file:///")
        .unwrap_or_else(|| panic!("AttachmentPreview made a URL that is not file:///: {url:?}"));
    // Reserved chars are the assertion: Qt may decode a space back.
    assert!(
        !path.contains('#') && !path.contains('?'),
        "AttachmentPreview left '#' or '?' in the path, so the URL points at a \
         different file: {url}"
    );
    assert!(
        path.contains("100%25") && path.contains("%231%3F.png"),
        "AttachmentPreview did not encode the name's percent, fragment and \
         query characters: {url}"
    );
    assert!(
        path.contains("/blobs/"),
        "AttachmentPreview encoded the slashes too, so the path is one segment: {url}"
    );

    assert_eq!(value("avatar-load"), "ok", "Avatar did not load. {context}");
    let url = value("avatar-url");
    assert!(
        url.starts_with("image://piirit/face?file=") && !url.contains('#'),
        "the avatar's picture is not one encoded value of a face's recipe: {url}"
    );
    // Image.Ready is 1: provider found the file.
    assert_eq!(
        value("avatar-status"),
        "1",
        "a picture saved under a name with '%', '#', '?', '&' and '=' in it \
         was not found through the avatar's URL. {context}"
    );
}
