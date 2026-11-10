import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * How messages and chat list look, what leaves and stays on this phone. From SettingsPage.
 * Controls write on tap; deletion period asks first (deletes at once, with
 * count).
 */
Page {
    id: page

    /// Unused: every settings page takes it (SettingsPage pushes it).
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

    /// Choice index; unknown is balanced, as core.
    function qualityIndex(quality) {
        return quality === 1 ? 1 : 0
    }

    /// Download limits in bytes, as parla. First: smallest core takes (near
    /// never). Last: no limit.
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
        //: Deleting old messages from this phone is turned off. As a chat's own
        //: disappearing messages setting says it.
        case 0: return qsTr("Off")
        case 1: return qsTr("After 1 hour")
        case 2: return qsTr("After 1 day")
        case 3: return qsTr("After 1 week")
        case 4: return qsTr("After 5 weeks")
        default: return qsTr("After 1 year")
        }
    }

    /// Period index, or -1 off-list: blank, not a false "Off".
    function periodIndex(seconds) {
        for (var i = 0; i < periods.length; i++) {
            if (periods[i] === seconds) {
                return i
            }
        }
        return -1
    }

    /// Reset choice to stored setting: Silica tap writes currentIndex, detaching
    /// binding (as DisappearingMessages).
    function refresh() {
        qualityCombo.currentIndex = page.qualityIndex(Settings.mediaQuality)
        downloadCombo.currentIndex = page.limitIndex(Settings.downloadLimit)
        deletionCombo.currentIndex = page.periodIndex(Settings.deleteDeviceAfter)
    }

    Connections {
        target: Settings
        onMediaQualityChanged: page.refresh()
        onDownloadLimitChanged: page.refresh()
        onDeleteDeviceAfterChanged: page.refresh()
    }

    Component.onCompleted: page.refresh()

    /// Deletion period picked. Off: set outright. Else deletes at once in all
    /// chats: core counts, reader confirms on own page; stored value till then.
    function choosePeriod(seconds) {
        if (seconds === 0) {
            Settings.deleteDeviceAfter = 0
            return
        }
        // Silica moved choice already.
        page.refresh()
        if (seconds === Settings.deleteDeviceAfter) {
            return
        }
        page.pendingPeriod = seconds
        core.estimate_auto_deletion(seconds)
    }

    /// Period awaiting answer, 0 none. Estimate by signal; earlier ask's ignored.
    property int pendingPeriod: 0
    /// Counted, not yet asked: `{ seconds, count }`, or null.
    property var pendingAnswer: null

    property string errorMessage: ""

    /// Ask once page on screen, stack idle. Six periods: Silica lists them on
    /// own page, count lands while it pops; push mid-transition refused
    /// (stderr only), choice lost. As QuickActionSetting's picker.
    function askDeletion() {
        if (!page.pendingAnswer || page.status !== PageStatus.Active || pageStack.busy) {
            return
        }
        var chosen = page.pendingAnswer.seconds
        var count = page.pendingAnswer.count
        page.pendingAnswer = null
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

    onStatusChanged: page.askDeletion()

    Connections {
        target: pageStack
        onBusyChanged: page.askDeletion()
    }

    Connections {
        target: core
        onAuto_deletion_estimated: {
            if (seconds !== page.pendingPeriod) {
                return
            }
            page.pendingPeriod = 0
            page.pendingAnswer = { seconds: seconds, count: count }
            page.askDeletion()
        }
        // Core refused setting or count: shown here.
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
                title: qsTr("Messages")
            }

            // Core recodes pictures, camera records video at chosen quality.
            // Choices and names as reference clients.
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

            // Above are core's; this only formats what arrives.
            TextSwitch {
                objectName: "markdownSwitch"
                text: qsTr("Show Markdown formatting")
                //: Under "Show Markdown formatting". The marks stay as typed:
                //: they are what a sender writes.
                description: qsTr("Formats *bold*, _italic_ and `code` in messages.")
                // Tap writes setting; setting moves switch.
                automaticCheck: false
                checked: Settings.markdownMode === 0
                onClicked: Settings.markdownMode = checked ? 1 : 0
            }

            TextSwitch {
                objectName: "chatListProfileSwitch"
                text: qsTr("Show your profile in the chat list")
                //: Under "Show your profile in the chat list".
                description: qsTr("Shows your profile picture and name at the top of the chat list.")
                automaticCheck: false
                checked: Settings.chatListProfile === true
                onClicked: Settings.chatListProfile = !checked
            }

            SectionHeader {
                //: Heading over what the phone itself keeps of messages.
                text: qsTr("On this phone")
            }

            // Core's `delete_device_after`: all chats, this phone only (chat
            // timer is per chat, agreed by members).
            ComboBox {
                id: deletionCombo
                objectName: "deletionCombo"
                width: parent.width
                label: qsTr("Delete old messages")
                // One sentence per string (docs/PROJECT.md).
                description: qsTr("Messages are deleted from this phone only, in every profile.")
                             + " " + qsTr("Other devices and chat members keep their copies.")

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
        }

        VerticalScrollDecorator {}
    }

    // Core refusal or failed deletion count.
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
