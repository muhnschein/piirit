import QtQuick 2.0
import Sailfish.Silica 1.0

// Silica file browser, used filtered to one extension. App reads chosen file;
// sets filter and heading. Test assigns first.
Page {
    property var nameFilters: []
    property string title: ""
    property var selectedContentProperties: ({ filePath: "" })
}
