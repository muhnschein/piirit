pragma Singleton
import QtQuick 2.0

// Silica's StandardPaths singleton names the user's folders on a device.
// Writable here, so a test can point a page at a directory of its own.
QtObject {
    property string documents: "/tmp/piiri-stub-standardpaths/Documents"
    property string music: "/tmp/piiri-stub-standardpaths/Music"
    property string pictures: "/tmp/piiri-stub-standardpaths/Pictures"
    property string videos: "/tmp/piiri-stub-standardpaths/Videos"
    property string download: "/tmp/piiri-stub-standardpaths/Downloads"
}
