import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Profile the chat list shows, and the way to each profile's page.
 *
 * Core keeps all accounts open: switching points chat list at another, no
 * startup. Chat list tells core which is current (opened next time); picking
 * current does nothing. Row menu: own page, invite code, backup, delete. Code
 * and backup here, not on profile page: showing code shouldn't need settings.
 *
 * Add: one plus under last row. Not three (would read as three profiles): the
 * three ways are asked on the next page (AddProfilePage.qml).
 *
 * Delete countdown beside list (PendingRemoval): rows go on every reload and
 * took the countdown with them (second of two quick deletes never ran). List
 * also refreshed in place (core.rs): no flicker.
 */
Page {
    id: page

    /// Profile the chat list is on.
    property int currentAccountId: 0

    // Deleting last profile: window (piirit.qml) owns first-profile setup; this
    // page dies in the same swipe.
    Connections {
        target: core
        onAccount_error: page.errorMessage = message
        // Row names profile's address, which moves when its relay goes; re-read
        // on relay change.
        onCore_event: {
            if (kind === "TransportsModified") {
                core.refresh_accounts()
            }
        }
    }

    property string errorMessage: ""

    /// Profile wait before removal, ms; tests lower it.
    property alias pendingDelay: doomedProfiles.delay

    /// Profiles asked to delete, undo window.
    PendingRemoval {
        id: doomedProfiles
        onRemove: core.remove_account(id)
    }

    // Page pushed over or swipe back: anything waiting goes now (timer may
    // not have fired).
    onStatusChanged: {
        if (page.status === PageStatus.Deactivating) {
            doomedProfiles.flush()
        }
    }

    // Again on actual destroy: "leaving" and "told it is leaving" differ. A
    // delete tapped during back gesture comes after `Deactivating`; its wait
    // would die with page. Flushing twice is harmless.
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

            /// Profile on its way out.
            readonly property bool doomed: doomedProfiles.pending(model.account_id)

            /// Silica countdown over profile. Removal in `doomedProfiles`: row
            /// goes on reload. Draws, reports tap.
            function raiseRemorse() {
                remorse.active = true
                //: What Silica's countdown says it is doing, over a
                //: profile the reader has asked to delete.
                remorse.item.execute(
                    body, qsTr("Deleting profile"), function() {},
                    doomedProfiles.countdownFor(model.account_id))
            }

            // Built on first delete request, not with row.
            Loader {
                id: remorse
                active: false
                sourceComponent: RemorseItem {
                    objectName: "profileRemorse"
                    onCanceled: doomedProfiles.spare(model.account_id)
                }
            }

            // Row rebuilt mid-wait comes back with no countdown.
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
                    // Invite code, second device, backup: per profile, take
                    // row's account (core invite, provider, export are per
                    // account).
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
                                // Forward swipe after device takes profile:
                                // chats the app is on (not necessarily this).
                                currentAccountId: page.currentAccountId
                            })
                    }
                    MenuItem {
                        objectName: "backupItem"
                        text: qsTr("Back up profile")
                        onClicked: pageStack.push(Qt.resolvedUrl("BackupPage.qml"), {
                            accountId: model.account_id,
                            // Forward swipe after backup: chats the app is on
                            // (not necessarily this).
                            currentAccountId: page.currentAccountId
                        })
                    }
                    MenuItem {
                        objectName: "deleteProfileItem"
                        text: qsTr("Delete profile")
                        // Page told, not row: row dies on reload.
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
                // Profile's own picture and colour, as core lists.
                ownColor: model.color
                picturePath: model.avatar_path
                // Reader's own; tells two profiles apart.
                showAddress: true
                isKeyContact: true
                // Room for badge and mark: long name fades before them.
                trailingSpace: marks.width + Theme.paddingMedium
            }

            Row {
                id: marks
                // Beside row, not in it: remorse doesn't cover it, so faded
                // to match.
                opacity: profileDelegate.doomed ? 0 : 1
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: body.verticalCenter
                }
                spacing: Theme.paddingMedium

                // Unread in this profile: core count, muted chats excluded.
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
                        // A number, pinned like all model reads.
                        textFormat: Text.PlainText
                        text: model.unread_count > 99 ? "99+" : model.unread_count
                    }
                }

                // Current profile, marked as chosen group member is.
                Label {
                    objectName: "currentMark"
                    anchors.verticalCenter: parent.verticalCenter
                    visible: model.account_id === page.currentAccountId
                    text: "✓"
                    color: Theme.highlightColor
                    font.pixelSize: Theme.fontSizeLarge
                }
            }

            // Profile waiting to go: remorse takes tap and calls deletion off.
            onClicked: {
                if (model.account_id !== page.currentAccountId) {
                    // Whole stack: `replace` alone left previous chat list
                    // under accounts page (one swipe back to old profile). Null
                    // target replaces all, as onboarding does.
                    pageStack.replaceAbove(null,
                                           Qt.resolvedUrl("ChatListPage.qml"),
                                           { accountId: model.account_id })
                } else {
                    pageStack.pop()
                }
            }
        }

        // One row for another profile; three ways behind it, not three pluses.
        footer: PlusRow {
            objectName: "addProfileButton"
            width: listView.width
            text: qsTr("Add profile")
            onClicked: pageStack.push(Qt.resolvedUrl("AddProfilePage.qml"), {})
        }

        // Counted off model: plus is view's own row; empty list keeps it.
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
