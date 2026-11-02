import QtQuick 2.0
import Piirit 1.0
import "."

/*
 * App's one `Transcriber`: settings and power in, core events through. Loaded by
 * piirit.qml (Loader: window loads without shim types in tests). Pages reach it as
 * `appWindow.transcriber`.
 */
Item {
    id: root

    readonly property QtObject transcriber: engine

    Transcriber {
        id: engine
        objectName: "transcriber"
        // `=== true`, `|| ""`, `|| 0`: dconf undefined before read.
        enabled: Settings.transcriptionEnabled === true
        languages: Settings.transcriptionLanguages || ""
        mode: Settings.transcriptionMode || 0
        older: Settings.transcriptionOlder === true
        charging: power.charging
        power_saving: power.saving
    }

    PowerWatch {
        id: power
        objectName: "powerWatch"
    }

    Connections {
        target: core
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onCore_event: engine.handle_event(context_id, kind, payload_json)
    }
}
