import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piiri 1.0

/*
 * QR codes, both ways: this profile's invite drawn as a code, for a phone
 * held up to this one, and the scanner for someone else's -- a switch at
 * the top picks which, the way an inline view switcher does on the
 * desktop. Reached from the chat list's pull-down, and from the profile
 * page's "Show invite code".
 *
 * The code is how anyone gets in touch with a profile: an address alone
 * cannot be encrypted to (docs/PROJECT.md), and a chatmail address is
 * nothing anybody types. The link the code carries is there to copy into
 * anything else. Someone else's code is read by the scanner, which is
 * also where a link can be typed or pasted, so nothing is pasted here.
 *
 * The code is an Image of a file the shim writes rather than a Canvas:
 * a Canvas is drawn into the window's GL context, which the platform
 * takes away from an app in the background, and it came back blank; an
 * Image is reloaded from its file. The scanner is loaded by URL when its
 * side is picked: ScanView.qml is the only file that names a Camera, so
 * a device without one costs that side rather than this page.
 *
 * Whatever the scanner reads is followed as an invite, and the chat it
 * leads to replaces this page, above the one that opened it.
 */
Page {
    id: page

    property int accountId
    property string errorMessage: ""
    property string myInvite: ""
    property bool joining: false
    /// Which side is showing: 0 this profile's code, 1 the scanner.
    property int mode: 0

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        onError: {
            page.joining = false
            page.errorMessage = message
        }
        onInvite_ready: page.myInvite = link
        onChat_ready: {
            page.joining = false
            pageStack.replaceAbove(pageStack.previousPage(page),
                                   Qt.resolvedUrl("ConversationPage.qml"), {
                accountId: page.accountId,
                chatId: chat_id,
                chatName: qsTr("Chat")
            })
        }
    }

    Component.onCompleted: contacts.fetch_invite()

    Connections {
        target: core
        // A model created before the core is up has nothing to load from.
        onStatus_changed: {
            if (core.status === "ready") {
                contacts.fetch_invite()
            }
        }
    }

    /// What the scanner read, followed. The scanner stays up, busy, until
    /// the chat replaces the page or the error lands.
    function follow(text) {
        page.errorMessage = ""
        page.joining = true
        contacts.join_by_invite(text)
    }

    // The invite as a picture, for a phone held up to this one.
    QrCode {
        id: qr
        objectName: "qr"
        text: page.myInvite
    }

    // Whose code this is: with more than one profile, the code side says
    // which of them it is showing.
    Profile {
        id: profile
        objectName: "profile"
        account_id: page.accountId
    }

    SilicaFlickable {
        id: flickable
        anchors.fill: parent
        contentHeight: Math.max(height, column.height + Theme.paddingLarge)

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingLarge

            PageHeader {
                title: qsTr("QR code")
            }

            // The switch, drawn the way Silica's own tab bar draws one:
            // the two names side by side across the page, the chosen one
            // in the highlight colour with a bar under it, a hairline
            // under both.
            Item {
                id: switcher
                objectName: "viewSwitcher"
                width: parent.width
                height: Theme.itemSizeSmall

                Row {
                    anchors.fill: parent

                    Repeater {
                        model: [qsTr("My code"), qsTr("Scan")]

                        BackgroundItem {
                            id: option
                            objectName: "viewOption" + index
                            width: switcher.width / 2
                            height: switcher.height

                            Label {
                                anchors.centerIn: parent
                                color: page.mode === index || option.highlighted
                                       ? Theme.highlightColor : Theme.primaryColor
                                text: modelData
                            }

                            Rectangle {
                                anchors {
                                    left: parent.left
                                    right: parent.right
                                    bottom: parent.bottom
                                    leftMargin: Theme.paddingLarge
                                    rightMargin: Theme.paddingLarge
                                }
                                height: Theme.paddingSmall / 2
                                visible: page.mode === index
                                color: Theme.highlightColor
                            }

                            onClicked: page.mode = index
                        }
                    }
                }

                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.rgba(Theme.primaryColor, 0.2)
                }
            }

            // This profile's code.
            Column {
                id: codeView
                objectName: "codeView"
                visible: page.mode === 0
                width: parent.width
                spacing: Theme.paddingLarge

                // Whose: the profile the code belongs to, drawn as the
                // profiles page draws it.
                ContactRow {
                    objectName: "profileRow"
                    width: parent.width
                    displayName: profile.display_name.length > 0
                                 ? profile.display_name : profile.address
                    address: profile.address
                    showAddress: true
                    ownColor: profile.color
                    picturePath: profile.avatar_path
                    isKeyContact: true
                }

                Label {
                    objectName: "intro"
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.secondaryHighlightColor
                    text: qsTr("Let someone scan this code, or send them the link.")
                }

                // Drawn on white whatever the ambience: a code is read by
                // contrast, and an ambience can have little. Not
                // smoothed: the modules stay square edges rather than
                // blur into each other.
                Image {
                    objectName: "inviteQr"
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: Math.floor(parent.width * 0.7)
                    height: width
                    visible: qr.size > 0
                    source: qr.image
                    smooth: false
                    cache: false
                    fillMode: Image.PreserveAspectFit
                }

                Label {
                    objectName: "myInviteLabel"
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    wrapMode: Text.WrapAnywhere
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    textFormat: Text.PlainText
                    text: page.myInvite.length > 0 ? page.myInvite
                                                   : qsTr("Fetching...")
                }

                Button {
                    objectName: "copyButton"
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("Copy Invite Link")
                    enabled: page.myInvite.length > 0
                    onClicked: Clipboard.text = page.myInvite
                }
            }

            // Someone else's, through the camera. The rest of the page,
            // down to the bottom.
            Item {
                id: scanArea
                objectName: "scanArea"
                visible: page.mode === 1
                width: parent.width
                height: visible
                        ? Math.max(Theme.itemSizeLarge,
                                   flickable.height - y - Theme.paddingLarge)
                        : 0

                Loader {
                    id: scanLoader
                    objectName: "scanLoader"
                    anchors.fill: parent
                    // Loaded only while its side is showing: the camera
                    // goes with it when the reader switches back.
                    active: page.mode === 1
                    source: Qt.resolvedUrl("../components/ScanView.qml")
                    onLoaded: {
                        scanLoader.item.scanned.connect(page.follow)
                        scanLoader.item.failed.connect(function(message) {
                            page.errorMessage = message
                        })
                    }
                }

                // The camera runs while this page and this side are on
                // screen, and stops while an invite is being followed.
                Binding {
                    target: scanLoader.item
                    property: "active"
                    value: page.status === PageStatus.Active && page.mode === 1
                           && !page.joining
                }

                Label {
                    objectName: "noCamera"
                    visible: scanLoader.status === Loader.Error
                    anchors.centerIn: parent
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    color: Theme.secondaryHighlightColor
                    text: qsTr("The camera is not available on this device.")
                }
            }
        }
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }
}
