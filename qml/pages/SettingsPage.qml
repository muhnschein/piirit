import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * The settings that belong to no profile: how a message is drawn, what
 * goes out with a link, how much of a picture or a video leaves with it,
 * how much of an attachment arrives unasked, how long a message is kept,
 * whether anything is announced and how much a notification gives away
 * and whether a muted group can still raise one, and, under Advanced,
 * the way to the cover's quick actions (set up on a page of their own,
 * QuickActionsPage.qml) and whether webxdc apps are offered at all.
 * Reached from the chat list's pull-down. A profile's own settings --
 * picture, name, address, read receipts, what the relay says, what it
 * takes -- are on the profile's page, reached from its row on the
 * profiles page; that row also carries the two things about a profile
 * that are not settings, its invite code and its backup, since the
 * core's export does one account at a time.
 *
 * The one exception to that division is the way into the block list,
 * under Privacy with the link cleaning, which is the other setting about
 * what the reader gives away. Blocking is the core's, and the core keeps it per
 * account, so the page it opens (BlockedContactsPage.qml) is a profile's
 * -- it is here because that is where both reference clients keep it,
 * and because a reader looking for it looks in the app's settings rather
 * than in a profile's. The profile is `accountId`, handed in by the chat
 * list this was pulled down from, which is the profile whose chats the
 * reader was looking at.
 *
 * The values live in dconf, behind the `Settings` singleton every page
 * reads (qml/components/Settings.qml); this page writes the same object,
 * so a change here reaches every open page without either side being
 * told. They were briefly a page in the system's Settings app instead,
 * which never showed up on a device: the entry that puts a page there is
 * outside the paths Harbour allows, and it turned out to need more than
 * that entry to appear at all.
 *
 * Nothing here needs saving: each control writes its setting on the tap.
 * One is chosen on a page of its own instead: the deletion period
 * deletes messages the moment it is set, so that one asks first, with
 * the count of what would go.
 */
Page {
    id: page

    /// Whose block list the Privacy row opens. Nothing else on this page
    /// belongs to a profile; see the note above.
    property int accountId

    /// The outgoing media qualities, as the core numbers them and as
    /// both reference clients offer them: balanced, or smaller and
    /// worse.
    function qualityLabel(index) {
        if (index === 1) {
            //: Outgoing media quality: smaller pictures and videos,
            //: which cost the reader and whoever they write to less
            //: data. Both reference clients' words for it.
            return qsTr("Lower quality, less data")
        }
        //: Outgoing media quality: what the core picks by default.
        return qsTr("Balanced")
    }

    /// Which choice a quality is. Anything the core does not know is
    /// balanced, which is what it falls back to itself.
    function qualityIndex(quality) {
        return quality === 1 ? 1 : 0
    }

    /// The download limits offered, in bytes, as parla offers them. The
    /// first is the smallest the core accepts, which is as near to never
    /// as it goes; the last is no limit.
    readonly property var limits: [32768, 262144, 524288, 1048576, 2097152, 5242880, 0]

    function limitLabel(index) {
        switch (index) {
        case 0: return qsTr("Never")
        case 1: return qsTr("Up to 256 kB")
        case 2: return qsTr("Up to 512 kB")
        case 3: return qsTr("Up to 1 MB")
        case 4: return qsTr("Up to 2 MB")
        case 5: return qsTr("Up to 5 MB")
        default: return qsTr("Always")
        }
    }

    function limitIndex(bytes) {
        for (var i = 0; i < limits.length; i++) {
            if (limits[i] === bytes) {
                return i
            }
        }
        return 3
    }

    /// The deletion periods offered, in seconds, as deltachat-android
    /// offers them: never, an hour, a day, a week, five weeks, a year.
    readonly property var periods: [0, 3600, 86400, 604800, 3024000, 31536000]

    function periodLabel(index) {
        switch (index) {
        case 0: return qsTr("Never")
        case 1: return qsTr("After 1 hour")
        case 2: return qsTr("After 1 day")
        case 3: return qsTr("After 1 week")
        case 4: return qsTr("After 5 weeks")
        default: return qsTr("After 1 year")
        }
    }

    /// Which choice a period is, or -1 for one that is not on the list:
    /// a blank rather than "Never" over messages that are going.
    function periodIndex(seconds) {
        for (var i = 0; i < periods.length; i++) {
            if (periods[i] === seconds) {
                return i
            }
        }
        return -1
    }

    function notificationIndex(detail) {
        return detail >= 0 && detail <= 2 ? detail : 0
    }

    function openQuickActions() {
        pageStack.push(Qt.resolvedUrl("QuickActionsPage.qml"), {
            accountId: page.accountId
        })
    }

    /// Put each choice back to what the setting holds. Silica writes
    /// currentIndex itself on a tap, which detaches a binding, so the
    /// choice is put back from the setting each time it changes -- the
    /// arrangement DisappearingMessages uses.
    function refresh() {
        qualityCombo.currentIndex = page.qualityIndex(Settings.mediaQuality)
        downloadCombo.currentIndex = page.limitIndex(Settings.downloadLimit)
        deletionCombo.currentIndex = page.periodIndex(Settings.deleteDeviceAfter)
        notificationCombo.currentIndex =
            page.notificationIndex(Settings.notificationDetail)
    }

    Connections {
        target: Settings
        onMediaQualityChanged: page.refresh()
        onDownloadLimitChanged: page.refresh()
        onDeleteDeviceAfterChanged: page.refresh()
        onNotificationDetailChanged: page.refresh()
    }

    Component.onCompleted: page.refresh()

    /// The reader picked a deletion period.
    ///
    /// Off is set outright: it deletes nothing. Anything else deletes
    /// every message older than it the moment the core hears of it, in
    /// every chat, so the core is asked how many that is first, and the
    /// answer is put to the reader on a page of their own before the
    /// setting is written. Until they agree the choice shown goes back
    /// to what the setting holds; it follows the setting once they do,
    /// and a dialog cancelled leaves it where it was.
    function choosePeriod(seconds) {
        if (seconds === 0) {
            Settings.deleteDeviceAfter = 0
            return
        }
        // Silica has already moved the choice to what was tapped.
        page.refresh()
        if (seconds === Settings.deleteDeviceAfter) {
            return
        }
        page.pendingPeriod = seconds
        core.estimate_auto_deletion(seconds)
    }

    /// The period waiting on the reader's answer, 0 for none. The
    /// estimate comes back by signal, and an answer to an earlier
    /// question is not the one to act on.
    property int pendingPeriod: 0

    property string errorMessage: ""

    Connections {
        target: core
        onAuto_deletion_estimated: {
            if (seconds !== page.pendingPeriod) {
                return
            }
            page.pendingPeriod = 0
            var chosen = seconds
            var dialog = pageStack.push(Qt.resolvedUrl("AutoDeleteDialog.qml"), {
                seconds: chosen,
                periodLabel: page.periodLabel(page.periodIndex(chosen)),
                count: count
            })
            if (dialog) {
                dialog.accepted.connect(function() {
                    Settings.deleteDeviceAfter = chosen
                })
            }
        }
        // A setting the core would not take, or a count it could not
        // give: said here, where the reader asked.
        onCore_error: page.errorMessage = message
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Settings")
            }

            SectionHeader {
                text: qsTr("Messages")
            }

            // What leaves the phone, above what arrives on it. The core
            // recodes a picture as it sends, and the camera records a
            // video, at whichever of these two the reader picks; both
            // reference clients offer the same pair under the same name.
            ComboBox {
                id: qualityCombo
                objectName: "qualityCombo"
                width: parent.width
                //: Pictures and videos on their way out of the phone.
                label: qsTr("Outgoing media quality")

                menu: ContextMenu {
                    MenuItem {
                        objectName: "qualityOption0"
                        text: page.qualityLabel(0)
                        onClicked: Settings.mediaQuality = 0
                    }
                    MenuItem {
                        objectName: "qualityOption1"
                        text: page.qualityLabel(1)
                        onClicked: Settings.mediaQuality = 1
                    }
                }
            }

            ComboBox {
                id: downloadCombo
                objectName: "downloadCombo"
                width: parent.width
                label: qsTr("Auto-download attachments")

                menu: ContextMenu {
                    Repeater {
                        model: page.limits

                        MenuItem {
                            objectName: "downloadOption" + modelData
                            text: page.limitLabel(index)
                            onClicked: Settings.downloadLimit = modelData
                        }
                    }
                }
            }

            // The core's own `delete_device_after`, which it applies to
            // every chat whatever that chat's disappearing messages timer
            // says -- that timer is the chat's, agreed between its
            // members; this is the phone's, and only the phone's. Under
            // Messages with the rest of what happens to one, rather than
            // under a heading of its own.
            ComboBox {
                id: deletionCombo
                objectName: "deletionCombo"
                width: parent.width
                label: qsTr("Delete messages from device")
                //: "Saved messages" is the name of the chat with oneself.
                description: qsTr("Applies to every chat, regardless of its disappearing messages setting. \"Saved messages\" are kept.")

                menu: ContextMenu {
                    Repeater {
                        model: page.periods

                        MenuItem {
                            objectName: "deletionOption" + modelData
                            text: page.periodLabel(index)
                            onClicked: page.choosePeriod(modelData)
                        }
                    }
                }
            }

            // Last under Messages: the three above are what the core does
            // with a message -- what leaves, what arrives, what stays --
            // and this is only how the app draws one.
            TextSwitch {
                objectName: "markdownSwitch"
                text: qsTr("Use Markdown formatting")
                // Checked follows the setting, so the tap writes the
                // setting and the setting moves the switch.
                automaticCheck: false
                checked: Settings.markdownMode === 0
                onClicked: Settings.markdownMode = checked ? 1 : 0
            }

            SectionHeader {
                text: qsTr("Notifications")
            }

            // First, because it decides whether the two below it mean
            // anything: off, nothing is announced and they are greyed out
            // rather than hidden, so the reader sees what comes back.
            TextSwitch {
                objectName: "notificationsSwitch"
                text: qsTr("Show notifications")
                automaticCheck: false
                checked: Settings.notificationsEnabled === true
                onClicked: Settings.notificationsEnabled = !checked
            }

            // What a notification says is what the lock screen shows to
            // whoever is looking at it, so the reader chooses how much.
            ComboBox {
                id: notificationCombo
                objectName: "notificationCombo"
                width: parent.width
                enabled: Settings.notificationsEnabled === true
                label: qsTr("Notification content")

                menu: ContextMenu {
                    MenuItem {
                        objectName: "notificationOption0"
                        text: qsTr("Sender and message")
                        onClicked: Settings.notificationDetail = 0
                    }
                    MenuItem {
                        objectName: "notificationOption1"
                        text: qsTr("Sender only")
                        onClicked: Settings.notificationDetail = 1
                    }
                    MenuItem {
                        objectName: "notificationOption2"
                        text: qsTr("No details")
                        onClicked: Settings.notificationDetail = 2
                    }
                }
            }

            // Muting a group silences it; this is the one thing that
            // still gets through, when the reader wants it to. The
            // reference clients' name for it, and their default.
            TextSwitch {
                objectName: "mentionsSwitch"
                //: A reply to one of the reader's own messages, arriving
                //: in a group they have muted.
                text: qsTr("Mentions")
                description: qsTr("In muted groups, notify messages directed to you, like replies or reactions")
                enabled: Settings.notificationsEnabled === true
                automaticCheck: false
                checked: Settings.mentionNotifications === true
                onClicked: Settings.mentionNotifications = !checked
            }

            // What the reader gives away, in one place: what leaves with
            // a link they send, and who is not heard from at all. The
            // link switch had a heading of its own with nothing else
            // under it.
            SectionHeader {
                text: qsTr("Privacy")
            }

            TextSwitch {
                objectName: "cleanLinksSwitch"
                text: qsTr("Remove tracking from links")
                description: qsTr("Removes click IDs and campaign tags from links you send.")
                // Bound to the setting, not held here, so the switch cannot
                // drift from what the app will read.
                automaticCheck: false
                checked: Settings.cleanLinks === true
                onClicked: Settings.cleanLinks = !checked
            }

            // The one row on this page that leads somewhere rather than
            // setting something, so it goes under the switches: the block
            // list is a list of people, and that is a page
            // (BlockedContactsPage.qml). It is where the reference clients
            // put it -- both keep it in the app's settings -- and it is
            // the one thing here that belongs to a profile rather than to
            // the phone, since the core keeps a block list per account. No
            // line under it saying which profile: this is pulled down from
            // that profile's chats, and every other page reached that way
            // is that profile's too.
            BackgroundItem {
                id: blockedEntry
                objectName: "blockedContactsEntry"
                width: parent.width
                height: Theme.itemSizeSmall

                Label {
                    objectName: "blockedContactsLabel"
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    truncationMode: TruncationMode.Fade
                    color: blockedEntry.highlighted ? Theme.highlightColor
                                                    : Theme.primaryColor
                    text: qsTr("Blocked contacts")
                }

                onClicked: pageStack.push(Qt.resolvedUrl("BlockedContactsPage.qml"), {
                    accountId: page.accountId
                })
            }

            // What most readers never need: the cover's quick actions,
            // set up on a page of their own (QuickActionsPage.qml) since
            // a chat's action brings a chat and a row of icons with it,
            // and webxdc apps.
            SectionHeader {
                //: The settings most readers never need to change.
                text: qsTr("Advanced")
            }

            BackgroundItem {
                id: quickActionsEntry
                objectName: "quickActionsEntry"
                width: parent.width
                height: Theme.itemSizeSmall

                Label {
                    objectName: "quickActionsLabel"
                    x: Theme.horizontalPageMargin
                    width: parent.width - 2 * Theme.horizontalPageMargin
                    anchors.verticalCenter: parent.verticalCenter
                    truncationMode: TruncationMode.Fade
                    color: quickActionsEntry.highlighted ? Theme.highlightColor
                                                         : Theme.primaryColor
                    text: qsTr("Quick actions")
                }

                onClicked: page.openQuickActions()
            }

            // Off until it is asked for, so this is the only place the
            // word webxdc appears on a phone that has not asked. What it
            // turns on is three things at once -- the tray's app entry,
            // the store behind it, and running one somebody sent -- and
            // each of them reads the setting rather than being told, so
            // there is nothing to keep in step here.
            TextSwitch {
                objectName: "webxdcSwitch"
                //: A webxdc app is a small program somebody sends into a
                //: chat and everyone in it plays with. Keep the name:
                //: it is what every other Delta Chat client calls them.
                text: qsTr("Enable webxdc apps (experimental)")
                description: qsTr("Runs small apps inside chats. These features may be unstable and may be changed or removed.")
                automaticCheck: false
                checked: Settings.webxdcEnabled === true
                onClicked: Settings.webxdcEnabled = !checked
            }
        }
    }

    // What went wrong, when something did: the core refusing a setting,
    // or unable to count what a deletion period would take.
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
