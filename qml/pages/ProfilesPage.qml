import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Which profile the chat list is showing, and the way to each one's own
 * page.
 *
 * The core keeps every configured account open at once, with IO running
 * on all of them, so switching is a matter of pointing the chat list at a
 * different one rather than starting anything up; the chat list tells the
 * core which it is on, and that is the profile the app opens on next
 * time. Picking the profile already shown does nothing. A row's menu is
 * everything to do with that profile: its own page -- picture, name,
 * address, the rest -- its invite code, writing it out to a backup file,
 * and deleting it. The code and the backup are here, where the profile is
 * picked, rather than on the profile's own page: a reader who wants to
 * show their code does not want to read a settings page first.
 *
 * Another profile is added from the plus under the last row, where the
 * group pages put "add members". One plus, not three: the three ways in
 * -- a new profile, a backup file, another device -- are the question the
 * page behind it asks (AddProfilePage.qml), and three pluses under a
 * list of profiles read as three more profiles.
 *
 * Deleting counts down beside the list rather than on the row
 * (PendingRemoval): the row goes whenever the list reloads, and it used
 * to take the countdown with it, so the second of two profiles deleted
 * in the same breath was never deleted at all. The list is also
 * refreshed in place rather than rebuilt when a deletion lands
 * (core.rs), which is worth keeping for its own sake -- it is what stops
 * every row flickering -- but the countdown no longer depends on it.
 */
Page {
    id: page

    /// The profile the chat list is currently on.
    property int currentAccountId: 0

    // Deleting the last profile leaves the app with nothing to show, and
    // the way back to where a first profile is made belongs to the window
    // (piirit.qml) rather than to this page: this page is destroyed by
    // the same swipe that asks for the deletion, so a move made from here
    // would be a move by a page that is no longer there.
    Connections {
        target: core
        onAccount_error: page.errorMessage = message
        // A row names the relay its profile sends from. The core says
        // when a profile's relays change -- from its page over this one,
        // or from another device the profile is on -- and the rows are
        // read again.
        onCore_event: {
            if (kind === "TransportsModified") {
                core.refresh_accounts()
            }
        }
    }

    property string errorMessage: ""

    /// How long a profile waits before it goes, in milliseconds.
    /// Nothing sets it; a test turns it down rather than waiting.
    property alias pendingDelay: doomedProfiles.delay

    /// The profiles the reader has asked to delete, waiting out the
    /// moment in which they can say they did not mean it.
    PendingRemoval {
        id: doomedProfiles
        onRemove: core.remove_account(id)
    }

    // A profile page pushed over this one, or a swipe back to the chats:
    // either way, anything still waiting goes now. Leaving is exactly
    // when a timer has not fired yet.
    onStatusChanged: {
        if (page.status === PageStatus.Deactivating) {
            doomedProfiles.flush()
        }
    }

    // And again as the page is actually destroyed, because "leaving" and
    // "told it is leaving" are not the same moment. A profile tapped for
    // deletion while the back gesture is already under way is asked for
    // after this page has had its `Deactivating`, and the wait it armed
    // then dies with the page and the timer on it: the reader asked for
    // a profile to go and it quietly stayed. Flushing twice costs
    // nothing -- the first one leaves nothing behind.
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
            // Named per profile, so a test can find the one it means.
            objectName: "profileRow" + model.account_id
            contentHeight: body.height

            /// This profile is on its way out.
            readonly property bool doomed: doomedProfiles.pending(model.account_id)

            /// Silica's own countdown, drawn over the profile. The
            /// deletion is not its business -- that belongs to
            /// `doomedProfiles`, because a remorse item lives in the row
            /// it covers and the row goes whenever the list reloads. So
            /// it draws, and reports the tap.
            function raiseRemorse() {
                //: What Silica's countdown says it is doing, over a
                //: profile the reader has asked to delete.
                remorse.execute(
                    body, qsTr("Deleting profile"), function() {},
                    doomedProfiles.countdownFor(model.account_id))
            }

            RemorseItem {
                id: remorse
                objectName: "profileRemorse"
                onCanceled: doomedProfiles.spare(model.account_id)
            }

            // A row rebuilt mid-wait comes back with no countdown on it.
            Component.onCompleted: {
                if (profileDelegate.doomed) {
                    profileDelegate.raiseRemorse()
                }
            }

            menu: ContextMenu {
                MenuItem {
                    objectName: "profileSettingsItem"
                    text: qsTr("Profile settings")
                    onClicked: pageStack.push(Qt.resolvedUrl("ProfilePage.qml"),
                                              { accountId: model.account_id })
                }
                // The invite code, the second device and the backup are
                // about this profile and nothing else, so they are on
                // the profile rather than a page deeper in. All three
                // take the row's own account: the core's invite, its
                // provider and its export are each per account.
                MenuItem {
                    objectName: "inviteItem"
                    text: qsTr("Invite code")
                    onClicked: pageStack.push(Qt.resolvedUrl("QrPage.qml"),
                                              { accountId: model.account_id })
                }
                MenuItem {
                    objectName: "secondDeviceItem"
                    text: qsTr("Add a second device")
                    // The dialog, not the page: the code the page shows
                    // is the profile, so the warning about who can see
                    // it has to come before the code is on screen.
                    onClicked: pageStack.push(
                        Qt.resolvedUrl("SecondDeviceDialog.qml"), {
                            accountId: model.account_id,
                            // Where the forward swipe goes once a
                            // device has taken the profile: the chats
                            // the app is on, which is not necessarily
                            // the profile being offered.
                            currentAccountId: page.currentAccountId
                        })
                }
                MenuItem {
                    objectName: "backupItem"
                    text: qsTr("Back up profile")
                    onClicked: pageStack.push(Qt.resolvedUrl("BackupPage.qml"), {
                        accountId: model.account_id,
                        // Where the forward swipe goes once the backup
                        // is written: the chats the app is on, which is
                        // not necessarily the profile being backed up.
                        currentAccountId: page.currentAccountId
                    })
                }
                MenuItem {
                    objectName: "deleteProfileItem"
                    text: qsTr("Delete profile")
                    // The page is told, not this row: the row is
                    // destroyed whenever the list reloads, and a wait
                    // living on it would go too. Same as the chat list
                    // and the conversation.
                    onClicked: {
                        doomedProfiles.ask(model.account_id)
                        profileDelegate.raiseRemorse()
                    }
                }
            }

            ContactRow {
                id: body
                // The remorse covering this does the fading, with its
                // own `opacity: 0.0` on what it was handed.
                enabled: !profileDelegate.doomed
                width: parent.width
                displayName: model.display_name.length > 0
                             ? model.display_name : model.addr
                address: model.addr
                // The profile's own picture and colour, as the core lists
                // them with the account: the same row the chat list draws
                // for everyone else, drawn for oneself.
                ownColor: model.color
                picturePath: model.avatar_path
                // The reader's own, and what tells two profiles apart.
                showAddress: true
                isKeyContact: true
                // Room for the badge and the mark, so a long name fades
                // before them rather than running under them.
                trailingSpace: marks.width + Theme.paddingMedium
            }

            Row {
                id: marks
                // Beside the row rather than in it, so the remorse does
                // not cover it: faded to match.
                opacity: profileDelegate.doomed ? 0 : 1
                anchors {
                    right: parent.right
                    rightMargin: Theme.horizontalPageMargin
                    verticalCenter: body.verticalCenter
                }
                spacing: Theme.paddingMedium

                // Waiting to be read in this profile: the badge the chat
                // list draws on a chat, drawn on the profile. The core's
                // own count, which leaves muted chats out.
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
                        // A number, but pinned like everything read off
                        // a model.
                        textFormat: Text.PlainText
                        text: model.unread_count > 99 ? "99+" : model.unread_count
                    }
                }

                // The one being shown, marked the way a chosen group
                // member is.
                Label {
                    objectName: "currentMark"
                    anchors.verticalCenter: parent.verticalCenter
                    visible: model.account_id === page.currentAccountId
                    text: "✓"
                    color: Theme.highlightColor
                    font.pixelSize: Theme.fontSizeLarge
                }
            }

            // A profile waiting to go is covered by the remorse, which
            // takes the tap itself and calls the deletion off.
            onClicked: {
                if (model.account_id !== page.currentAccountId) {
                    // The whole stack, not just this page. `replace`
                    // swapped out the accounts page and left the previous
                    // account's chat list underneath it -- one swipe back
                    // into the profile just left. A null target replaces
                    // everything, which is what the onboarding pages do
                    // when they hand over to the chat list.
                    pageStack.replaceAbove(null,
                                           Qt.resolvedUrl("ChatListPage.qml"),
                                           { accountId: model.account_id })
                } else {
                    pageStack.pop()
                }
            }
        }

        // The way to another profile, where the next one would be
        // listed: a row shaped like a profile's, with a plus for a
        // picture, as the group pages offer another member. Under the
        // last row rather than in the pulley, which is where a reader
        // who has just read the list is already looking.
        //
        // One row, and the three ways in are behind it. Three pluses in a
        // column -- make one, read a backup file, join from another device
        // -- would put three answers under a list of profiles before the
        // reader had been asked anything.
        footer: ListItem {
            id: addProfileRow
            objectName: "addProfileButton"
            width: listView.width
            contentHeight: Theme.itemSizeSmall + 2 * Theme.paddingMedium

            PlusMark {
                id: plus
                x: Theme.horizontalPageMargin
                y: Theme.paddingMedium
            }

            Label {
                x: plus.x + plus.width + Theme.paddingMedium
                width: parent.width - x - Theme.horizontalPageMargin
                anchors.verticalCenter: plus.verticalCenter
                wrapMode: Text.Wrap
                color: addProfileRow.highlighted ? Theme.highlightColor
                                                 : Theme.primaryColor
                text: qsTr("Add profile")
            }

            onClicked: pageStack.push(Qt.resolvedUrl("AddProfilePage.qml"), {})
        }

        // Counted off the model, not off what is drawn: the plus is the
        // view's own row rather than a profile, so a list with nothing
        // in it still has a row on it.
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
