import QtQuick 2.0

// Silica's switch: an icon under a light that is lit while it is on.
// `icon` is an alias for the reason IconButton's is. A tap the harness
// raises with clicked() turns it over, as Silica's own does, unless the
// page has taken that over (`automaticCheck: false`).
Item {
    property alias icon: iconImage
    Image { id: iconImage; visible: false }
    property bool checked: false
    property bool automaticCheck: true
    property bool busy: false
    signal clicked()
    onClicked: {
        if (automaticCheck) {
            checked = !checked
        }
    }
    width: 80
    height: 100
}
