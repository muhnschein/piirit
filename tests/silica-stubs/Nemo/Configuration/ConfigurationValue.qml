import QtQuick 2.0

// Stub: device dconf value. Stores nothing; holds page writes, test reads back.
QtObject {
    property string key
    property var defaultValue
    property var value: defaultValue
}
