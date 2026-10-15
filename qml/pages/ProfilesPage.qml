import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Pick chat list's profile; way to each profile's page.
 *
 * Core keeps all accounts open: switch = repoint chat list, which tells core
 * current (opened next time). Row menu: page, invite code, backup, delete;
 * code and backup here so showing code needs no settings.
 *
 * One plus under last row (three would read as profiles); ways asked on
 * AddProfilePage.qml.
 *
 * Delete countdown outside rows (PendingRemoval): reload killed it with row
 * (second quick delete never ran). List refreshed in place (core.rs).
 */
Page {
    id: page

    /// Profile the chat list is on.
    property int currentAccountId: 0

    // Last profile deleted: window (piirit.qml) does setup; page dies.
    Connections {
        target: core
        onAccount_error: page.errorMessage = message
        // Row shows address, which moves with relay: re-read.
        onCore_event: {
            if (kind === "TransportsModified") {
                core.refresh_accounts()
            }
        }
    }

    property string errorMessage: ""

    /// Removal wait, ms; tests lower.
    property alias pendingDelay: doomedProfiles.delay

    /// Profiles asked to delete, undo window.
    PendingRemoval {
        id: doomedProfiles
        onRemove: core.remove_account(id)
    }

    // Pushed over or swiped back: pending deletes go now.
    onStatusChanged: {
        if (page.status === PageStatus.Deactivating) {
            doomedProfiles.flush()
        }
    }

    // Again on destroy: delete tapped mid back gesture comes after
    // `Deactivating`, would die with page. Double flush harmless.
    Component.onDestruction: doomedProfiles.flush()

    SilicaListView {
        id: listView
        objectName: "profileList"
        anchors.fill: parent
        model: core.account_list

        header: PageHeader {
            title: qsTr("Profiles")
        }

        delegate: ListItem {
            id: profileDelegate
            // Named per profile for tests.
            objectName: "profileRow" + model.account_id
            contentHeight: body.height

            /// Pending removal.
            readonly property bool doomed: doomedProfiles.pending(model.account_id)

            /// Silica countdown; state in `doomedProfiles` (row dies on reload).
            function raiseRemorse() {
                remorse.active = true
                //: What Silica's countdown says it is doing, over a
                //: profile the reader has asked to delete.
                remorse.item.execute(
                    body, qsTr("Deleting profile"), function() {},
                    doomedProfiles.countdownFor(model.account_id))
            }

            // Built on first delete.
            Loader {
                id: remorse
                active: false
                sourceComponent: RemorseItem {
                    objectName: "profileRemorse"
                    onCanceled: doomedProfiles.spare(model.account_id)
                }
            }

            // Row rebuilt mid-wait: no countdown.
            Component.onCompleted: {
                if (profileDelegate.doomed) {
                    profileDelegate.raiseRemorse()
                }
            }

            // Component, built on first menu open; see ConversationList.
            menu: Component {
                ContextMenu {
                    MenuItem {
                        objectName: "profileSettingsItem"
                        text: qsTr("Profile settings")
                        onClicked: pageStack.push(Qt.resolvedUrl("ProfilePage.qml"),
                                                  { accountId: model.account_id })
                    }
                    // Invite code, second device, backup: row's account.
                    MenuItem {
                        objectName: "inviteItem"
                        text: qsTr("Invite code")
                        onClicked: pageStack.push(Qt.resolvedUrl("QrPage.qml"),
                                                  { accountId: model.account_id })
                    }
                    MenuItem {
                        objectName: "secondDeviceItem"
                        text: qsTr("Add a second device")
                        onClicked: pageStack.push(
                            Qt.resolvedUrl("SecondDevicePage.qml"), {
                                accountId: model.account_id,
                                // Forward after takeover: current chats.
                                currentAccountId: page.currentAccountId
                            })
                    }
                    MenuItem {
                        objectName: "backupItem"
                        text: qsTr("Back up profile")
                        onClicked: pageStack.push(Qt.resolvedUrl("BackupPage.qml"), {
                            accountId: model.account_id,
                            // Forward after backup: current chats.
                            currentAccountId: page.currentAccountId
                        })
                    }
                    MenuItem {
                        objectName: "deleteProfileItem"
                        text: qsTr("Delete profile")
                        // Tell page: row dies on reload.
                        onClicked: {
                            doomedProfiles.ask(model.account_id)
                            profileDelegate.raiseRemorse()
                        }
                    }
                }
            }

            ContactRow {
                id: body
                // Remorse fades via own `opacity: 0.0`.
                enabled: !profileDelegate.doomed
                width: parent.width
                displayName: model.display_name.length > 0
                             ? model.display_name : model.addr
                address: model.addr
                // Profile's picture and colour.
                ownColor: model.color
                picturePath: model.avatar_path
                // Reader's own: tells profiles apart.
                showAddress: true
                isKeyContact: true
                // Long name fades before badge and mark.
                trailingSpace: marks.width + Theme.paddingMedium
            }

            Row {
                id: marks
                // Outside row, remorse doesn't cover: faded to match.
                opacity: profileDelegate.doomed ? 0 : 1
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: body.verticalCenter
                }
                spacing: Theme.paddingMedium

                // Core unread count, muted excluded.
                Rectangle {
                    objectName: "profileUnreadBadge"
                    anchors.verticalCenter: parent.verticalCenter
                    visible: model.unread_count > 0
                    width: visible
                           ? Math.max(height, unreadLabel.implicitWidth + Theme.paddingMedium)
                           : 0
                    height: unreadLabel.implicitHeight + Theme.paddingSmall
                    radius: height / 2
                    color: Theme.highlightColor

                    Label {
                        id: unreadLabel
                        objectName: "profileUnreadLabel"
                        anchors.centerIn: parent
                        font.pixelSize: Theme.fontSizeExtraSmall
                        color: Theme.primaryColor
                        // Pinned as all model reads.
                        textFormat: Text.PlainText
                        text: model.unread_count > 99 ? "99+" : model.unread_count
                    }
                }

                // Current: marked as chosen group member.
                Label {
                    objectName: "currentMark"
                    anchors.verticalCenter: parent.verticalCenter
                    visible: model.account_id === page.currentAccountId
                    text: "✓"
                    color: Theme.highlightColor
                    font.pixelSize: Theme.fontSizeLarge
                }
            }

            // Pending delete: tap cancels.
            onClicked: {
                if (model.account_id !== page.currentAccountId) {
                    // Null target replaces whole stack (as onboarding):
                    // `replace` left old chat list one swipe back.
                    pageStack.replaceAbove(null,
                                           Qt.resolvedUrl("ChatListPage.qml"),
                                           { accountId: model.account_id })
                } else {
                    pageStack.pop()
                }
            }
        }

        // One add row; ways behind it.
        footer: PlusRow {
            objectName: "addProfileButton"
            width: listView.width
            text: qsTr("Add profile")
            onClicked: pageStack.push(Qt.resolvedUrl("AddProfilePage.qml"), {})
        }

        // Plus outside model: kept on empty list.
        ViewPlaceholder {
            enabled: listView.count === 0
            text: qsTr("No profiles")
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
        onDismissed: page.errorMessage = ""
    }
}
