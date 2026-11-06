import QtQuick 2.0
import Sailfish.Silica 1.0

/*
 * Map with nothing on it: core has no location from the day the map app shows, so the app
 * isn't started (MapOpener.qml).
 */
Page {
    id: page

    property string heading: ""
    property string subheading: ""

    SilicaFlickable {
        anchors.fill: parent

        PageHeader {
            objectName: "header"
            title: page.heading
            description: page.subheading
        }

        ViewPlaceholder {
            objectName: "placeholder"
            enabled: true
            //: Map of a chat's or every chat's shared locations, when there are none.
            text: qsTr("No locations yet")
            //: Under "No locations yet": the map shows only the last day's locations.
            hintText: qsTr("Locations shared in the last 24 hours show here.")
        }
    }
}
