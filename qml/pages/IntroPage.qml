import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * What Delta Chat is: five facts, one per swiped screen.
 * Wording from delta.chat/en/help, tech half cut (FAQ has it).
 * Swipe past last fact: setup. First screen swipes back too.
 * No faces field: pictures over facts are the look. Masks from
 * components/InkArt.qml, painted by tools/faces/scenes.py.
 */
Page {
    id: page

    // Columns either orientation.
    allowedOrientations: Orientation.All

    // Set past end: one drag cannot ask twice.
    property bool leaving: false

    /// Facts in order. Plain array: ListElement takes literals only;
    /// all translated.
    readonly property var facts: [
        {
            picture: "../art/intro-profile.png",
            title: qsTr("No sign-up, no phone number"),
            body: qsTr("Your profile is created on this phone. No phone number and no sign-up are needed.")
        },
        {
            picture: "../art/intro-invite.png",
            title: qsTr("You choose who can reach you"),
            body: qsTr("There is no public directory. People reach you by scanning your invite code or opening your invite link.")
        },
        {
            picture: "../art/intro-lock.png",
            title: qsTr("Every message is encrypted"),
            body: qsTr("Messages are end-to-end encrypted, so the relays that carry them cannot read them.")
        },
        {
            picture: "../art/intro-group.png",
            title: qsTr("Groups have no admins"),
            body: qsTr("Any member can add people, rename the group or leave it.")
        },
        {
            picture: "../art/intro-relay.png",
            title: qsTr("Relays only pass messages on"),
            body: qsTr("A relay stores a message only until the recipient fetches it.")
        }
    ]

    /// How far past last fact a drag must go to ask for next page.
    readonly property real enough: Theme.itemSizeMedium

    // Setup starts after facts. replace: back from setup shows first screen.
    function goOn() {
        if (page.leaving) {
            return
        }
        page.leaving = true
        pageStack.replace(Qt.resolvedUrl("ProfileStartPage.qml"), {})
    }

    // Asks on move, not after rubber band settles.
    function advanceIfPastEnd() {
        if (slides.width <= 0 || slides.contentWidth <= 0) {
            return
        }
        if (slides.contentX - (slides.contentWidth - slides.width) > page.enough) {
            page.goOn()
        }
    }

    ListView {
        id: slides
        objectName: "slides"
        anchors.fill: parent
        orientation: ListView.Horizontal
        // One fact at a time; one always on screen.
        snapMode: ListView.SnapOneItem
        highlightRangeMode: ListView.StrictlyEnforceRange
        // Last fact pullable: that asks for next page.
        boundsBehavior: Flickable.DragOverBounds
        model: page.facts

        // Only drag asks. Rotation resizes view: briefly measured against
        // old width, read as pull past last fact and left page untouched.
        onContentXChanged: {
            if (slides.dragging) {
                page.advanceIfPastEnd()
            }
        }
        // Turn leaves half of two facts: snap back to one.
        onWidthChanged: slides.positionViewAtIndex(slides.currentIndex,
                                                   ListView.Beginning)

        delegate: Item {
            width: slides.width
            height: slides.height

            Column {
                anchors {
                    horizontalCenter: parent.horizontalCenter
                    verticalCenter: parent.verticalCenter
                    // Slightly above middle: dots below.
                    verticalCenterOffset: -Theme.itemSizeSmall / 2
                }
                width: parent.width - 2 * Theme.horizontalPageMargin
                spacing: Theme.paddingLarge

                InkArt {
                    objectName: "slideArt" + index
                    anchors.horizontalCenter: parent.horizontalCenter
                    // Square, small: room for words on sideways screen.
                    width: Math.min(page.width * 0.42, page.height * 0.30)
                    height: width
                    source: modelData.picture
                }

                Label {
                    objectName: "slideTitle" + index
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: modelData.title
                    font.family: Theme.fontFamilyHeading
                    font.pixelSize: Theme.fontSizeLarge
                    color: Theme.highlightColor
                }

                Label {
                    objectName: "slideBody" + index
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    text: modelData.body
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.primaryColor
                }
            }
        }
    }

    // Reader position; on last fact, what one more swipe does.
    Column {
        anchors {
            horizontalCenter: parent.horizontalCenter
            top: parent.verticalCenter
            topMargin: page.height * 0.25
        }
        width: page.width - 2 * Theme.horizontalPageMargin
        spacing: Theme.paddingLarge

        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: Theme.paddingMedium

            Repeater {
                model: page.facts.length

                Rectangle {
                    width: Theme.paddingSmall
                    height: width
                    radius: width / 2
                    color: index === slides.currentIndex ? Theme.highlightColor
                                                         : Theme.primaryColor
                    opacity: index === slides.currentIndex ? 1.0 : 0.3
                }
            }
        }

        Label {
            objectName: "swipeHint"
            width: parent.width
            visible: slides.currentIndex === page.facts.length - 1
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            font.pixelSize: Theme.fontSizeExtraSmall
            color: Theme.secondaryHighlightColor
            text: qsTr("Keep swiping to set up your profile.")

            // Tap does what line says. For the thumb that finds it, not a
            // second pull.
            MouseArea {
                anchors.fill: parent
                onClicked: page.goOn()
            }
        }
    }
}
