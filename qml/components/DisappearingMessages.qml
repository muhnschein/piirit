import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Message lifetime: nine reference-client values, off to a year. Tap asks
 * core; ComboBox detaches binding on tap, so page restores it each load.
 */
Column {
    id: root

    /// Seconds from core. 0 is off.
    property int seconds: 0
    /// Choice offered.
    property bool canChange: true

    /// Reader picked seconds.
    signal chosen(int seconds)

    // Reference clients' order.
    readonly property var choices: [0, 60, 300, 1800, 3600, 86400, 604800, 3024000, 31536000]

    /// Duration in largest fitting unit. Hand forms: untranslated %n shows "(s)".
    function words(value) {
        if (value <= 0) {
            return qsTr("Off")
        }
        var units = [
            [31536000, qsTr("After 1 year"), qsTr("After %1 years")],
            [604800, qsTr("After 1 week"), qsTr("After %1 weeks")],
            [86400, qsTr("After 1 day"), qsTr("After %1 days")],
            [3600, qsTr("After 1 hour"), qsTr("After %1 hours")],
            [60, qsTr("After 1 minute"), qsTr("After %1 minutes")],
            [1, qsTr("After 1 second"), qsTr("After %1 seconds")]
        ]
        for (var i = 0; i < units.length; i++) {
            if (value >= units[i][0]) {
                var count = value / units[i][0]
                if (count === 1) {
                    return units[i][1]
                }
                var shown = count === Math.floor(count) ? count : count.toFixed(1)
                return units[i][2].arg(shown)
            }
        }
        return qsTr("Off")
    }

    function labelFor(index) {
        return root.words(root.choices[index])
    }

    function indexOf(value) {
        for (var i = 0; i < choices.length; i++) {
            if (choices[i] === value) {
                return i
            }
        }
        return -1
    }

    /// Reset to core value.
    function refresh() {
        combo.currentIndex = root.indexOf(root.seconds)
    }

    onSecondsChanged: refresh()
    Component.onCompleted: refresh()

    width: parent ? parent.width : 0

    ComboBox {
        id: combo
        objectName: "disappearingCombo"
        width: parent.width
        label: qsTr("Disappearing messages")
        enabled: root.canChange
        // Same wording on or off list.
        value: root.words(root.seconds)

        menu: ContextMenu {
            Repeater {
                model: root.choices

                MenuItem {
                    objectName: "timerOption" + modelData
                    text: root.labelFor(index)
                    onClicked: root.chosen(modelData)
                }
            }
        }
    }

    Label {
        objectName: "disappearingNote"
        x: Theme.horizontalPageMargin
        width: parent.width - 2 * Theme.horizontalPageMargin
        wrapMode: Text.Wrap
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryColor
        text: qsTr("Applies to all members of this chat; they can still copy, save, and forward messages.")
    }
}
