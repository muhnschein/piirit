import QtQuick 2.0

// The real one hands its actions to the home screen, which draws the
// first list that is enabled; here it only holds them, and says whether
// it is on.
QtObject {
    property bool enabled: true
    property bool iconBackground: false
    default property list<QtObject> actions
}
