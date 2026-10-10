pragma Singleton
import QtQuick 2.0

// The display, as Silica reports it: the stub Page's own size.
QtObject {
    property int width: 540
    property int height: 960
    // The notch: none here. A phone with one reports its rectangle.
    property rect topCutout: Qt.rect(0, 0, 0, 0)
}
