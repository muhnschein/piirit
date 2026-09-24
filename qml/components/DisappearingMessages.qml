import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * How long messages in a chat live: the same nine choices the reference
 * clients offer, from off to a year.
 *
 * The core is what decides. The chosen index is bound from it rather than
 * held here, and a tap asks the core for the other value; the binding then
 * shows whatever the core says -- the same arrangement as the read-receipt
 * switch, and for the same reason. Silica's ComboBox writes currentIndex
 * itself on a tap, which detaches the binding, so the page puts it back on
 * every load.
 *
 * A duration another client set that is not on the list is said in the
 * largest unit that fits it, so it reads as "After 1 year" rather than as
 * a count of seconds.
 */
Column {
    id: root

    /// Seconds, as the core holds them. 0 is off.
    property int seconds: 0
    /// Whether a choice is offered at all.
    property bool canChange: true

    /// The reader picked a duration, in seconds.
    signal chosen(int seconds)

    // In the reference clients' order.
    readonly property var choices: [0, 60, 300, 1800, 3600, 86400, 604800, 3024000, 31536000]

    /// A duration in words: the largest unit it is at least one of, with
    /// a decimal only when it is not a whole number of them. Two forms by
    /// hand rather than qsTr's %n: without a loaded translation %n shows
    /// the source text as it stands, "(s)" and all.
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

    /// Put the choice back to what the core holds.
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
        // On the list or not, the duration is said the same way.
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
