pragma Singleton
import QtQuick 2.0

// Display as Silica reports: stub Page's own size.
QtObject {
    property int width: 540
    property int height: 960
    // Notch: none. Phone with one reports its rectangle.
    property rect topCutout: Qt.rect(0, 0, 0, 0)
}
