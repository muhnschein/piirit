import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * The settings that belong to no profile: whether the return key sends,
 * how a message is drawn, what goes out with a link, how much of an
 * attachment arrives unasked, how long a message is kept, how much a
 * notification gives away and whether a muted group can still raise one,
 * and whether webxdc apps are offered at all. Reached from the chat
 * list's pull-down. A profile's own
 * settings -- picture, name, address, read receipts, what the relay says
 * -- are on the profile's page, reached from its row on the profiles page.
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
 * The one exception is the deletion period, which deletes messages the
 * moment it is set, so that one asks first -- on a page of its own, with
 * the count of what would go.
 */
Page {
    id: page

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

    /// Put each choice back to what the setting holds. Silica writes
    /// currentIndex itself on a tap, which detaches a binding, so the
    /// choice is put back from the setting each time it changes -- the
    /// arrangement DisappearingMessages uses.
    function refresh() {
        downloadCombo.currentIndex = page.limitIndex(Settings.downloadLimit)
        deletionCombo.currentIndex = page.periodIndex(Settings.deleteDeviceAfter)
        notificationCombo.currentIndex =
            page.notificationIndex(Settings.notificationDetail)
    }

    Connections {
        target: Settings
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

            // First, because it is the one setting the reader meets on
            // every message: what the biggest key on the keyboard does.
            // Off, the field is the multi-line one -- it grows with the
            // message and the button sends -- and the description says
            // so, since turning this on takes that away without saying.
            TextSwitch {
                objectName: "enterSendsSwitch"
                //: The return key on the keyboard.
                text: qsTr("Enter sends the message")
                description: qsTr("Off, the return key starts a new line, the message field grows with what is written, and the send button sends.")
                automaticCheck: false
                checked: Settings.enterSends === true
                onClicked: Settings.enterSends = !checked
            }

            TextSwitch {
                objectName: "markdownSwitch"
                text: qsTr("Markdown")
                description: qsTr("On, a message written with *stars* and `backticks` is shown with the bold, the italics and the links drawn. Off, it is shown exactly as it was written.")
                // Checked follows the setting, so the tap writes the
                // setting and the setting moves the switch -- the way
                // the switch above it works.
                automaticCheck: false
                checked: Settings.markdownMode === 0
                onClicked: Settings.markdownMode = checked ? 1 : 0
            }

            ComboBox {
                id: downloadCombo
                objectName: "downloadCombo"
                width: parent.width
                label: qsTr("Auto-download attachments")
                description: qsTr("Bigger ones wait until you ask for them. Applies to every profile and to messages that arrive from now on.")

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
                description: qsTr("Older messages go from this phone, in every chat of every profile, whatever a chat's own disappearing messages setting says. \"Saved messages\" are kept.")

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

            SectionHeader {
                text: qsTr("Notifications")
            }

            // What a notification says is what the lock screen shows to
            // whoever is looking at it, so the reader chooses how much.
            ComboBox {
                id: notificationCombo
                objectName: "notificationCombo"
                width: parent.width
                label: qsTr("A new notification shows")
                description: qsTr("On the lock screen and in the notification area. The chat it is from opens on a tap either way.")

                menu: ContextMenu {
                    MenuItem {
                        objectName: "notificationOption0"
                        text: qsTr("Who wrote, and what")
                        onClicked: Settings.notificationDetail = 0
                    }
                    MenuItem {
                        objectName: "notificationOption1"
                        text: qsTr("Who wrote")
                        onClicked: Settings.notificationDetail = 1
                    }
                    MenuItem {
                        objectName: "notificationOption2"
                        text: qsTr("Only that something arrived")
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
                description: qsTr("In a muted group, a reply to one of your messages still notifies you.")
                automaticCheck: false
                checked: Settings.mentionNotifications === true
                onClicked: Settings.mentionNotifications = !checked
            }

            SectionHeader {
                text: qsTr("Links")
            }

            TextSwitch {
                objectName: "cleanLinksSwitch"
                text: qsTr("Remove tracking from links")
                description: qsTr("Known tracking parameters -- click ids, campaign tags, the sharer's account -- are taken out of the links in the messages you send. The rest of the link is left as it was.")
                // Bound to the setting, not held here, so the switch cannot
                // drift from what the app will read.
                automaticCheck: false
                checked: Settings.cleanLinks === true
                onClicked: Settings.cleanLinks = !checked
            }

            SectionHeader {
                text: qsTr("Apps")
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
                description: qsTr("Apps somebody sends run inside the chat, and the attach tray offers a store to take new ones from. An app is somebody else's code, and this part is not yet as tested as the rest.")
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
