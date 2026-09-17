pragma Singleton
import QtQuick 2.0

// Silica's StandardPaths singleton names the user's folders on a device.
// Writable here, so a test can point a page at a directory of its own.
QtObject {
    property string documents: "/tmp/postivene-stub-standardpaths/Documents"
    property string music: "/tmp/postivene-stub-standardpaths/Music"
    property string pictures: "/tmp/postivene-stub-standardpaths/Pictures"
    property string videos: "/tmp/postivene-stub-standardpaths/Videos"
    property string download: "/tmp/postivene-stub-standardpaths/Downloads"
}
