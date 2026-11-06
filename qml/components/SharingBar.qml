import QtQuick 2.0
import Sailfish.Silica 1.0
import "."

/*
 * Chat gets reader's location live: says so above field, time left under it, stop beside.
 * Own file so it loads and tests alone (ConversationPage won't headless).
 */
Item {
    id: root

    /// Chat gets location now.
    property bool sharing: false
    /// Sharing ends then, Unix seconds; 0 unknown.
    property double until: 0
    /// Sharing, but no position going out (LocationFeed.qml).
    property bool stalled: false
    /// Ms since epoch; tests set it.
    property double now: Clock.now
    signal stopRequested()

    /// Minutes under an hour, rounded up; hours after, nearest. Empty: end unknown.
    readonly property string leftText: {
        var left = root.until - root.now / 1000
        if (root.until <= 0) {
            return ""
        }
        if (left < 3600) {
            //: Time until live location sharing ends, under an hour.
            return qsTr("%n minute(s) left", "", Math.max(1, Math.ceil(left / 60)))
        }
        //: Time until live location sharing ends, an hour or more.
        return qsTr("%n hour(s) left", "", Math.round(left / 3600))
    }

    visible: root.sharing
    height: root.sharing ? Math.max(label.height + (timeLeft.visible ? timeLeft.height : 0),
                                    stop.height) + 2 * Theme.paddingSmall
                         : 0

    Image {
        id: pin
        anchors.verticalCenter: parent.verticalCenter
        x: Theme.horizontalPageMargin
        width: Theme.iconSizeSmall
        height: Theme.iconSizeSmall
        source: "image://theme/icon-m-location?"
                + (root.stalled ? Theme.errorColor : Theme.highlightColor)
    }

    Label {
        id: label
        objectName: "sharingLabel"
        x: pin.x + pin.width + Theme.paddingSmall
        y: (root.height - label.height - (timeLeft.visible ? timeLeft.height : 0)) / 2
        width: parent.width - x - stop.width - Theme.paddingMedium
        wrapMode: Text.Wrap
        font.pixelSize: Theme.fontSizeExtraSmall
        color: root.stalled ? Theme.errorColor : Theme.highlightColor
        textFormat: Text.PlainText
        text: root.stalled
              //: Live location on, but the phone hands no position: nothing goes out.
              ? qsTr("Your location is not being sent. Is location turned on in Settings?")
              : qsTr("You are sharing your location")
    }

    Label {
        id: timeLeft
        objectName: "sharingLeft"
        visible: root.leftText.length > 0
        x: label.x
        y: label.y + label.height
        width: label.width
        font.pixelSize: Theme.fontSizeExtraSmall
        color: Theme.secondaryHighlightColor
        textFormat: Text.PlainText
        text: root.leftText
    }

    IconButton {
        id: stop
        objectName: "stopSharingButton"
        anchors {
            verticalCenter: parent.verticalCenter
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        icon.source: "image://theme/icon-m-clear"
        onClicked: root.stopRequested()
    }
}
