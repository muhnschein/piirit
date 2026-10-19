import QtQuick 2.0
import Sailfish.Silica 1.0

// `pageStack` not declared: context property injected by harness.
Item {
    // PageStatus.Active: page loaded alone is on screen.
    property int status: PageStatus.Active
    // True once page attached to right in Silica.
    property bool canNavigateForward: false
    property int allowedOrientations: 1
    property int orientation
    property bool backNavigation: true
    // Silica's colours for page's items; pages set scheme.
    property Palette palette: Palette {}
    width: 540
    height: 960
}
