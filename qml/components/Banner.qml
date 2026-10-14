import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Status strip: failure or confirm. Transient; page clears on `dismissed`.
 */
Item {
    id: root

    property string text: ""
    // "error" or "info": severity.
    property string tone: "error"
    // objectName for tests; 2 per page would clash.
    property string labelObjectName: "errorLabel"
    // Auto-clear seconds; 0 keeps.
    property int timeout: 8
    signal dismissed()

    readonly property color toneColor: root.tone === "error" ? Theme.errorColor
                                                             : Theme.highlightColor

    /// Show briefly. For cases lacking page state.
    function show(message) {
        root.text = message
        // Restart here: onTextChanged skips repeats; else they vanish at once.
        if (root.text.length > 0 && root.timeout > 0) {
            fade.restart()
        }
    }

    width: parent ? parent.width : 0
    height: root.text.length > 0 ? strip.height : 0
    visible: root.text.length > 0

    onTextChanged: {
        if (root.text.length > 0 && root.timeout > 0) {
            fade.restart()
        } else {
            fade.stop()
        }
    }

    Rectangle {
        id: strip
        width: parent.width
        height: label.implicitHeight + 2 * Theme.paddingMedium
        color: Theme.rgba(root.toneColor, 0.15)

        Label {
            id: label
            objectName: root.labelObjectName
            anchors {
                left: parent.left
                right: parent.right
                verticalCenter: parent.verticalCenter
                leftMargin: Theme.horizontalPageMargin
                rightMargin: Theme.horizontalPageMargin
            }
            textFormat: Text.PlainText
            text: root.text
            color: root.toneColor
            font.pixelSize: Theme.fontSizeSmall
            wrapMode: Text.Wrap
        }

        MouseArea {
            anchors.fill: parent
            onClicked: root.dismissed()
        }
    }

    Timer {
        id: fade
        interval: root.timeout * 1000
        onTriggered: root.dismissed()
    }
}
