import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import Piirit 1.0

/*
 * QR codes both ways: this profile's invite, and the scanner for others'. Top
 * switch picks side. Reached from chat list, profile, cover actions.
 *
 * Code is how anyone reaches a profile (address alone can't be encrypted to;
 * docs/PROJECT.md). Scanner also takes typed or pasted links.
 *
 * Code is an Image of a shim-written file, not Canvas: Canvas draws into
 * window's GL context, which platform takes from background apps; came back
 * blank. Image reloads from file. Scanner loaded by URL: ScanView.qml is the only Camera
 * user, so no camera costs that side only.
 *
 * Scanned invite: followed, chat replaces page (above opener). Relay codes
 * (dcaccount:, dclogin:): add-relay page replaces it, asks first.
 */
Page {
    id: page

    // Core move held until on screen: call page may cover it, and move then
    // would take that page (PendingNavigation.qml).
    PendingNavigation {
        id: navigation
        stack: pageStack
        ready: page.status === PageStatus.Active
    }

    /// No quick actions: jump away drops code mid-read. See piirit.qml.
    readonly property bool pausesQuickActions: true

    property int accountId
    property string errorMessage: ""
    property string myInvite: ""
    property bool joining: false
    /// Side showing: 0 this profile's code, 1 scanner.
    property int mode: 0
    /// App in front. Not read-only: tests (no app in front) set it.
    property bool appActive: Qt.application.state === Qt.ApplicationActive

    ContactList {
        id: contacts
        objectName: "contacts"
        account_id: page.accountId
        onError: {
            page.joining = false
            page.errorMessage = message
        }
        onInvite_ready: page.myInvite = link
        // Relay code, not invite: one more relay for this profile; replaces
        // page.
        onRelay_offered: {
            page.joining = false
            navigation.replaceAbove(pageStack.previousPage(page),
                                    Qt.resolvedUrl("AddRelayPage.qml"), {
                accountId: page.accountId,
                scannedQr: qr_content,
                scannedRelay: relay
            })
        }
        onChat_ready: {
            page.joining = false
            navigation.replaceAbove(pageStack.previousPage(page),
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
        // Model made before core up: nothing to load.
        onStatus_changed: {
            if (core.status === "ready") {
                contacts.fetch_invite()
            }
        }
    }

    /// Scanned text, followed. Scanner busy until chat replaces page or error.
    function follow(text) {
        page.errorMessage = ""
        page.joining = true
        contacts.join_by_invite(text)
    }

    // Invite as picture, for a phone held to this one.
    QrCode {
        id: qr
        objectName: "qr"
        text: page.myInvite
    }

    // Whose code: with several profiles, the code names which.
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

            // Switch drawn like Silica tab bar; chosen one highlighted.
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

                // Whose: profile the code belongs to.
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

                // White whatever ambience: codes read by contrast. Not
                // smoothed: modules keep square edges.
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
                                                   : qsTr("Fetching…")
                }

                Button {
                    objectName: "copyButton"
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: qsTr("Copy invite link")
                    enabled: page.myInvite.length > 0
                    onClicked: Clipboard.text = page.myInvite
                }
            }

            // Scanner through camera; rest of page down to bottom.
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
                    // Loaded only while side shows: camera goes when switched
                    // back.
                    active: page.mode === 1
                    source: Qt.resolvedUrl("../components/ScanView.qml")
                    onLoaded: {
                        scanLoader.item.scanned.connect(page.follow)
                        scanLoader.item.failed.connect(function(message) {
                            page.errorMessage = message
                        })
                    }
                }

                // Camera runs while page and side shown; stops while invite
                // followed, and in background or screen off (Silica keeps page
                // active then).
                Binding {
                    target: scanLoader.item
                    property: "active"
                    value: page.status === PageStatus.Active && page.appActive
                           && page.mode === 1 && !page.joining
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
