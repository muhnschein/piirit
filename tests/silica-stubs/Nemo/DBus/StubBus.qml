pragma Singleton
import QtQuick 2.0

// Test-only, not in real Nemo.DBus: session bus as adaptors leave it. `names`: service ->
// true; `objects`: path -> adaptor. Tests read it to tap as lipstick does.
QtObject {
    property var names: ({})
    property var objects: ({})
}
