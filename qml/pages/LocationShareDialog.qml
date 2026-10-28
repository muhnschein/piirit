import QtQuick 2.0
import Sailfish.Silica 1.0
import QtPositioning 5.2

/*
 * Location into chat: current place once, or live for a set time (reference clients' times).
 * One switch at a time, as AddProfileWaysDialog.qml. Page sends on `accepted`; position
 * looked for only while this is up.
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

    readonly property bool hasFix: source.position.latitudeValid
                                   && source.position.longitudeValid

    canAccept: dialog.choice.length > 0 && (dialog.choice !== "now" || dialog.hasFix)

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

    PositionSource {
        id: source
        objectName: "position"
        active: true
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

            // `automaticCheck: false`: one answer, dialog decides (AddProfileWaysDialog.qml).
            TextSwitch {
                objectName: "nowSwitch"
                text: qsTr("Send current location")
                // 0: PositionSource.NoError.
                description: dialog.hasFix
                             ? (source.position.horizontalAccuracyValid
                                //: %n is how far off, in metres, the phone's position may be.
                                ? qsTr("Your location is accurate to %n metre(s).", "",
                                       Math.round(source.position.horizontalAccuracy))
                                : "")
                             : source.sourceError !== 0 || !source.valid
                               ? qsTr("Turn on location in Settings to send it.")
                               : qsTr("Finding your location…")
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
