import QtQuick 2.0

// Real one hands actions to home screen (draws first enabled list). Here: holds
// them, reports enabled.
QtObject {
    property bool enabled: true
    property bool iconBackground: false
    default property list<QtObject> actions
}
