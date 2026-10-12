import QtQuick 2.0
import Sailfish.Silica 1.0

// `pageStack` is not declared here: it is a context property, which the
// harness injects.
Item {
    // PageStatus.Active: a page loaded on its own is the one on screen.
    property int status: PageStatus.Active
    // True in Silica once a page is attached to the right.
    property bool canNavigateForward: false
    property int allowedOrientations: 1
    property int orientation
    property string backNavigation
    width: 540
    height: 960
}
