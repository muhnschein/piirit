import QtQuick 2.0

// Silica switch: icon under light, lit while on. `icon` alias (as IconButton).
// Harness clicked() flips it, as Silica, unless page takes over
// (`automaticCheck: false`).
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
