import QtQuick 2.0

// Silica enum via QML `enum` (host Qt 5.15; device Qt 5.6 never sees stubs).
//
// `None` not Silica's: Label defaults to no truncation. Here so stub Label has
// "left alone".
QtObject {
    enum Mode { None, Fade, Elide }
}
