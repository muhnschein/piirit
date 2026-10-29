import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Location into chat: current place once, or live for a set time (reference clients' times).
 * One switch at a time, as AddProfileWaysDialog.qml. Page sends on `accepted`; position
 * looked for only while this is up. No fresh fix: nothing would go out, so only stop goes.
 */
Dialog {
    id: dialog

    /// Chat gets reader's location live now: stop offered.
    property bool sharing: false

    /// "now", "stop", or seconds as text.
    property string choice: ""

    /// Place once: read on accept.
    signal locationPicked(real latitude, real longitude)
    /// Live for `seconds`; 0 stops.
    signal sharingPicked(int seconds)

    /// Ms without fresh fix until location counted lost; tests shorten it.
    property int staleAfter: 15000

    canAccept: dialog.choice === "stop" || (dialog.choice.length > 0 && source.ready)

    onAccepted: {
        if (dialog.choice === "now") {
            dialog.locationPicked(source.position.coordinate.latitude,
                                  source.position.coordinate.longitude)
        } else if (dialog.choice === "stop") {
            dialog.sharingPicked(0)
        } else {
            dialog.sharingPicked(parseInt(dialog.choice, 10))
        }
    }

    function choose(which) {
        dialog.choice = dialog.choice === which ? "" : which
    }

    FreshPosition {
        id: source
        sourceName: "position"
        active: true
        staleAfter: dialog.staleAfter
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height

        Column {
            id: column
            width: dialog.width

            DialogHeader {
                objectName: "header"
                //: Sends the picked location, or starts or stops sharing it.
                acceptText: qsTr("Share")
            }

            Label {
                objectName: "locationStatus"
                visible: !source.ready
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                color: Theme.highlightColor
                textFormat: Text.PlainText
                text: source.failed
                      ? qsTr("Turn on location in Settings to send it.")
                      : source.lost
                        //: No position for a while: location likely turned off since.
                        ? qsTr("Still looking for your location. Is location turned on in Settings?")
                        : qsTr("Finding your location…")
            }

            // `automaticCheck: false`: one answer, dialog decides (AddProfileWaysDialog.qml).
            TextSwitch {
                objectName: "nowSwitch"
                text: qsTr("Send current location")
                description: source.ready && source.position.horizontalAccuracyValid
                             //: %n is how far off, in metres, the phone's position may be.
                             ? qsTr("Your location is accurate to %n metre(s).", "",
                                    Math.round(source.position.horizontalAccuracy))
                             : ""
                automaticCheck: false
                checked: dialog.choice === "now"
                onClicked: dialog.choose("now")
            }

            SectionHeader {
                //: Above the times for which the reader can share their location live.
                text: qsTr("Share live location")
            }

            Label {
                objectName: "liveNote"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                textFormat: Text.PlainText
                text: qsTr("Piirit keeps sending your location while it runs in the background.")
            }

            // Delta Chat Android's times and words.
            Repeater {
                model: [
                    { seconds: 1800, label: qsTr("For 30 minutes") },
                    { seconds: 3600, label: qsTr("For 1 hour") },
                    { seconds: 7200, label: qsTr("For 2 hours") },
                    { seconds: 21600, label: qsTr("For 6 hours") },
                    { seconds: 86400, label: qsTr("For 24 hours") }
                ]
                TextSwitch {
                    objectName: "liveSwitch"
                    readonly property string seconds: "" + modelData.seconds
                    text: modelData.label
                    automaticCheck: false
                    checked: dialog.choice === seconds
                    onClicked: dialog.choose(seconds)
                }
            }

            TextSwitch {
                objectName: "stopSwitch"
                visible: dialog.sharing
                text: qsTr("Stop sharing location")
                automaticCheck: false
                checked: dialog.choice === "stop"
                onClicked: dialog.choose("stop")
            }
        }
    }
}
