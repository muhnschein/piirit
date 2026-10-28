import QtQuick 2.0
import Sailfish.Silica 1.0
import Sailfish.WebView 1.0
import "../components"
import Piirit 1.0

/*
 * Row's location on OpenStreetMap's embed page: map and pin, nothing else of the site. Live
 * sharing's notice shows sender's newest; moves as core hears of them. Pushed by URL:
 * `Sailfish.WebView` resolves only where engine installed.
 */
Page {
    id: page

    property int accountId
    property int chatId
    /// Sender; their newest place if row has none of its own.
    property int contactId
    property int messageId
    property string senderName: ""

    property string errorMessage: ""

    /// Embed page and nothing past it: link taps go to browser.
    readonly property string mapPage: "https://www.openstreetmap.org/export/embed.html"

    SharedLocation {
        id: place
        objectName: "place"
        account_id: page.accountId
        chat_id: page.chatId
        contact_id: page.contactId
        message_id: page.messageId
        onError: page.errorMessage = message
    }

    Connections {
        target: core
        // Qt 5.6 handler syntax; see WelcomePage.qml.
        onCore_event: place.handle_event(context_id, kind, payload_json)
    }

    /// When sender was there, as the phone writes dates.
    readonly property string when: place.found
        ? Qt.formatDateTime(new Date(place.timestamp * 1000), Qt.DefaultLocaleShortDate)
        : ""

    // Not PageHeader: sender's name is peer text.
    ConversationHeader {
        id: header
        objectName: "locationHeader"
        title: page.senderName
        //: Under the sender's name above a map of their live location; %1 is a date and time.
        subtitle: place.live ? qsTr("Last seen here at %1").arg(page.when) : page.when
    }

    /// Keep view on map: anything else (attribution, larger map) opens in browser.
    function keepInside() {
        var here = "" + view.url
        if (here.length === 0 || here.indexOf(page.mapPage) === 0
                || here.indexOf("about:") === 0) {
            return
        }
        view.stop()
        Qt.openUrlExternally(here)
        view.url = Qt.binding(function () { return place.map_url })
    }

    WebView {
        id: view
        objectName: "mapView"
        visible: place.found
        anchors {
            top: header.bottom
            left: parent.left
            right: parent.right
            bottom: browserButton.top
            bottomMargin: Theme.paddingMedium
        }
        url: place.map_url
        onUrlChanged: page.keepInside()
    }

    BusyIndicator {
        objectName: "mapBusy"
        anchors.centerIn: view
        size: BusyIndicatorSize.Large
        running: !place.loaded && page.errorMessage.length === 0
    }

    Label {
        objectName: "mapEmpty"
        anchors {
            top: parent.verticalCenter
            left: parent.left
            leftMargin: Theme.horizontalPageMargin
            right: parent.right
            rightMargin: Theme.horizontalPageMargin
        }
        visible: (place.loaded && !place.found) || page.errorMessage.length > 0
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        color: Theme.secondaryHighlightColor
        textFormat: Text.PlainText
        text: page.errorMessage.length > 0 ? page.errorMessage
                                           : qsTr("No location has arrived yet.")
    }

    Button {
        id: browserButton
        objectName: "browserButton"
        anchors {
            horizontalCenter: parent.horizontalCenter
            bottom: parent.bottom
            bottomMargin: Theme.paddingLarge
        }
        enabled: place.found
        text: qsTr("Open in browser")
        onClicked: Qt.openUrlExternally(place.link)
    }
}
