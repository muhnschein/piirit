import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * No-profile settings: message look, link cleaning, media quality, auto
 * download, retention, notifications, quick actions (QuickActionsPage.qml),
 * webxdc. Opened from chat list. Profile settings on profile page.
 * Exception: block list row under Privacy (core keeps it per account, but both
 * reference clients and readers look here). `accountId` = chat list's profile.
 *
 * Values in dconf via `Settings` singleton (qml/components/Settings.qml);
 * changes reach open pages unannounced. Not a system Settings app page: its
 * entry is outside Harbour-allowed paths, and needed more than that to show.
 * Controls write on tap. Deletion period asks first (deletes at once, with
 * count).
 */
Page {
    id: page

    /// Profile for the block list row. Rest is per phone.
    property int accountId

    /// Outgoing media quality: balanced, or smaller and worse.
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

    /// Choice index. Unknown values are balanced, as core does.
    function qualityIndex(quality) {
        return quality === 1 ? 1 : 0
    }

    /// Download limits in bytes, as parla offers. First: smallest core takes
    /// (near never). Last: no limit.
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

    /// Deletion periods in seconds, as deltachat-android offers.
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

    /// Period index, or -1 if off-list: blank, not "Never" over messages that
    /// are going.
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

    /// Reset choice to stored setting on change. Silica writes currentIndex on tap,
    /// detaching the binding (same as DisappearingMessages).
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

    /// Reader picked a deletion period.
    ///
    /// Off: set outright (deletes nothing). Else deletes older messages in all
    /// chats at once: core counts them, reader confirms on own page first.
    /// Choice shows stored value until confirmed; cancel leaves it.
    function choosePeriod(seconds) {
        if (seconds === 0) {
            Settings.deleteDeviceAfter = 0
            return
        }
        // Silica already moved choice to tap.
        page.refresh()
        if (seconds === Settings.deleteDeviceAfter) {
            return
        }
        page.pendingPeriod = seconds
        core.estimate_auto_deletion(seconds)
    }

    /// Experimental switch: off at once; on only after reader accepts the risk.
    function toggleExperimental(toggle, setting) {
        if (toggle.checked) {
            Settings[setting] = false
            return
        }
        var dialog = pageStack.push(Qt.resolvedUrl("ExperimentalFeatureDialog.qml"), {
            title: toggle.text
        })
        if (dialog) {
            dialog.accepted.connect(function() {
                Settings[setting] = true
            })
        }
    }

    /// Period awaiting reader's answer, 0 for none. Estimate comes by signal;
    /// answer to an earlier ask is ignored.
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
        // Setting core refused, or count it could not give: shown here.
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

            // Outgoing media: core recodes pictures on send, camera records
            // video, at chosen quality. Same two choices, same names as
            // reference clients.
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

            // Core's `delete_device_after`: all chats, whatever their own timer
            // (that is per chat, agreed by members; this is this phone's only).
            ComboBox {
                id: deletionCombo
                objectName: "deletionCombo"
                width: parent.width
                label: qsTr("Delete messages from device")
                //: Under "Delete messages from device". "Disappearing
                //: messages" is each chat's own setting of that name.
                description: qsTr("This applies to every profile, whatever a chat's own disappearing messages setting is.")

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

            // Last under Messages: above three are core's; this only draws the message.
            TextSwitch {
                objectName: "markdownSwitch"
                text: qsTr("Use Markdown formatting")
                // Checked follows setting: tap writes it, setting moves switch.
                automaticCheck: false
                checked: Settings.markdownMode === 0
                onClicked: Settings.markdownMode = checked ? 1 : 0
            }

            SectionHeader {
                text: qsTr("Notifications")
            }

            // First: decides if the two below mean anything. Off: greyed, not hidden.
            TextSwitch {
                objectName: "notificationsSwitch"
                text: qsTr("Show notifications")
                automaticCheck: false
                checked: Settings.notificationsEnabled === true
                onClicked: Settings.notificationsEnabled = !checked
            }

            // Lock screen shows notification text, so reader chooses how much.
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

            // Only thing that gets through a muted group, when reader wants it.
            // Reference clients' name and default.
            // Calls from a contact: reference clients' setting, words, default.
            TextSwitch {
                objectName: "callsRingSwitch"
                //: Whether calls ring. A plural noun, not a verb.
                text: qsTr("Calls")
                description: qsTr("Show call screen for accepted contacts")
                automaticCheck: false
                checked: Settings.callsRing !== false
                onClicked: Settings.callsRing = !checked
            }

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

            // Gives away: link data sent, and who is not heard from.
            SectionHeader {
                text: qsTr("Privacy")
            }

            TextSwitch {
                objectName: "cleanLinksSwitch"
                text: qsTr("Remove tracking from links")
                description: qsTr("Removes click IDs and campaign tags from links you send.")
                // Bound to setting, not held here: switch can't drift from app.
                automaticCheck: false
                checked: Settings.cleanLinks === true
                onClicked: Settings.cleanLinks = !checked
            }

            // Only row opening a page, so under switches. Block list is per account:
            // profile's own (BlockedContactsPage.qml), kept here like reference clients.
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

            // Rarely needed: cover quick actions (QuickActionsPage.qml), webxdc apps.
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

            // Off until asked, so only place "webxdc" shows on a fresh phone.
            // Turns on tray entry, store, and running apps; each reads the setting.
            TextSwitch {
                id: webxdcSwitch
                objectName: "webxdcSwitch"
                //: A webxdc app is a small program somebody sends into a
                //: chat and everyone in it plays with. Keep the name:
                //: it is what every other Delta Chat client calls them.
                text: qsTr("Enable webxdc apps (experimental)")
                description: qsTr("Runs small apps inside chats.")
                automaticCheck: false
                checked: Settings.webxdcEnabled === true
                onClicked: page.toggleExperimental(webxdcSwitch, "webxdcEnabled")
            }

        }
    }

    // Core refusing a setting, or unable to count deletion, shown here.
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
