import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"
import "../js/Format.js" as Format
import Postivene 1.0

/*
 * One profile, as everyone else sees it and as this device holds it: the
 * picture, the name on every message, the line under it, the address it
 * writes from, whether the other end is told when something has been
 * read, and how the relay and the phone are doing by it. Reached from the
 * profile's row on the profiles page. The settings that belong to no
 * profile are on the settings page instead (SettingsPage.qml).
 *
 * This page is a profile's settings, so the two things about a profile
 * that are not settings are not here: the invite code to show and the backup to
 * write are on the profile's row on the profiles page, which is where
 * the profile is picked in the first place.
 *
 * The editable parts are core config keys rather than a record of their
 * own, so this page owns a Profile object over get_config/set_config
 * instead of a model. Nothing is confirmed: edits apply a moment after
 * typing stops and again on the way out, and the switch applies on the
 * tap. A settings page that needs saving is a settings page that loses
 * what was typed when someone swipes back.
 *
 * The name sits under the picture and wears the same edit badge; a tap
 * turns it into a field (EditableName.qml). The connectivity and the
 * mailbox quota follow parla's profile dialog (github.com/trufae/parla):
 * the core's own band for the connection, with a dot in the colour the
 * core's own report uses; the bar for the mailbox, always there, at
 * nothing until the relay has said; and under it what is used, what is
 * left and what there is, read off the core's report. What parla's
 * "Details" dialog adds -- the storage per conversation, scanned message
 * by message -- is not here: on a phone that scan is what the reader
 * would be waiting on.
 */
Page {
    id: page

    property int accountId

    Profile {
        id: profile
        objectName: "profile"
        account_id: page.accountId
        onError: page.errorMessage = message
        // The fields are only filled from the core, never re-filled from
        // it while someone is typing into them: a save reloads, and that
        // would otherwise reach in and reset the cursor.
        onLoaded_changed: {
            if (profile.loaded && !page.edited) {
                // Assigning to a field fires onTextChanged, which is the
                // same signal the reader typing produces. Without this
                // the load itself would count as an edit, and leaving
                // the page would write the profile back over itself.
                page.filling = true
                nameField.text = profile.display_name
                bioField.text = profile.status
                page.filling = false
            }
        }
    }

    Connections {
        target: core
        // The core says when the connection changes; the mailbox and
        // the storage are re-read with it.
        onCore_event: {
            if (kind === "ConnectivityChanged" && context_id === page.accountId) {
                profile.refresh_connectivity()
            }
        }
    }

    /// Someone has typed since the load. Guards the refill above.
    property bool edited: false
    /// The refill is writing to the fields, so the changes are not edits.
    property bool filling: false
    property string errorMessage: ""

    // A pause, not a keystroke: a round trip per letter would be four
    // calls to write "Ada".
    Timer {
        id: autosave
        objectName: "autosave"
        interval: 1200
        onTriggered: page.applyEdits()
    }

    function applyEdits() {
        if (!profile.loaded || !page.edited) {
            return
        }
        page.edited = false
        profile.save(nameField.text, bioField.text)
    }

    function noteEdit() {
        if (profile.loaded && !page.filling) {
            page.edited = true
            autosave.restart()
        }
    }

    // Leaving is the other moment worth saving at: a back-swipe within
    // the pause above would otherwise drop what was typed. The cursor
    // leaves the field on the way out, so the keyboard does not follow
    // the page.
    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            page.applyEdits()
            nameField.done()
        }
    }

    // The gallery, pushed by URL and connected to, the way the
    // conversation attaches a photo: the Attach*Page files are the only
    // ones that name a `Sailfish.Pickers` type, so a type that is not
    // there costs this button rather than the page. That page also
    // ignores a cancelled pick, so the core is never handed an
    // `undefined` path.
    function pickPicture() {
        var picker = pageStack.push(Qt.resolvedUrl("AttachPhotoPage.qml"))
        if (picker) {
            picker.picked.connect(function(path) {
                // The core copies the file into its own blob directory,
                // so the picked one may go away afterwards.
                profile.set_picture(path)
            })
        }
    }

    /// The core's connectivity band in words -- parla's, near enough.
    /// The bands are the core's (connectivity.rs); 0 is a profile not
    /// yet asked about.
    function connectionWords(state) {
        if (state >= 4000) {
            return qsTr("Connected, and up to date")
        }
        if (state >= 3000) {
            return qsTr("Connected, sending or syncing messages")
        }
        if (state >= 2000) {
            return qsTr("Connecting to the relay")
        }
        if (state >= 1000) {
            return qsTr("Not connected")
        }
        return qsTr("Checking the connection")
    }

    /// The dot beside the words, in the colours the core's own report
    /// draws its dots: green connected, yellow on the way, red not, and
    /// grey for not yet asked.
    function connectionColor(state) {
        if (state >= 4000) {
            return "#4caf50"
        }
        if (state >= 2000) {
            return "#ffc107"
        }
        if (state >= 1000) {
            return "#f44336"
        }
        return Theme.secondaryColor
    }

    /// A size, with nothing at all said as "0 B" rather than as nothing.
    function size(bytes) {
        return bytes > 0 ? Format.readableSize(bytes) : "0 B"
    }

    /// What is under the bar: the amounts when the relay gave them, the
    /// core's own sentence when it gave something this could not read,
    /// and the fact that it has not said yet otherwise.
    function quotaWords() {
        if (profile.quota_limit_bytes > 0) {
            var left = Math.max(0, profile.quota_limit_bytes - profile.quota_used_bytes)
            //: The mailbox on the relay. %1 used, %2 left, %3 the whole, each a size such as "1.4 GB".
            return qsTr("%1 used · %2 left of %3")
                .arg(page.size(profile.quota_used_bytes))
                .arg(page.size(left))
                .arg(page.size(profile.quota_limit_bytes))
        }
        if (profile.quota_text.length > 0) {
            return profile.quota_text
        }
        return qsTr("The relay has not reported its quota yet")
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        // Only there when there is a picture to remove, and kept off the
        // picture itself: a tap that might delete is not a tap you want
        // under a finger reaching for the gallery. The connection needs
        // no entry: the core says when it changes, and the page re-reads.
        PullDownMenu {
            visible: profile.avatar_path.length > 0

            MenuItem {
                objectName: "removePicture"
                text: qsTr("Remove picture")
                onClicked: profile.clear_picture()
            }
        }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("Profile")
            }

            // The picture is the control, not a preview of one: tapping
            // it opens the gallery, and the badge says so without a row
            // of its own.
            Item {
                width: parent.width
                height: bigAvatar.height + 2 * Theme.paddingLarge

                Avatar {
                    id: bigAvatar
                    objectName: "profileAvatar"
                    anchors.centerIn: parent
                    width: 2 * Theme.itemSizeExtraLarge
                    initial: nameField.text.length > 0
                             ? nameField.text
                             : profile.address
                    picturePath: profile.avatar_path
                }

                Rectangle {
                    id: editBadge
                    objectName: "editBadge"
                    anchors {
                        right: bigAvatar.right
                        bottom: bigAvatar.bottom
                    }
                    width: Theme.itemSizeExtraSmall
                    height: width
                    radius: width / 2
                    color: Theme.highlightBackgroundColor

                    Image {
                        anchors.centerIn: parent
                        source: "image://theme/icon-s-edit"
                    }
                }

                MouseArea {
                    objectName: "pictureTap"
                    anchors.fill: bigAvatar
                    onClicked: page.pickPicture()
                }
            }

            // The name on every message, under the picture. The field
            // is what is read and written; see EditableName.qml.
            EditableName {
                id: nameField
                objectName: "profileNameControl"
                fieldObjectName: "profileNameField"
                hintObjectName: "nameHint"
                placeholderText: qsTr("Your name")
                hint: qsTr("The name on every message you send")
                onTextChanged: page.noteEdit()
            }

            TextField {
                id: bioField
                objectName: "profileBioField"
                width: parent.width
                label: qsTr("Bio")
                placeholderText: qsTr("A line about you")
                onTextChanged: page.noteEdit()
            }

            // The reader's own address: what the relay minted, and what
            // tells two profiles apart. Shown, not edited -- changing it
            // is a different transport, not a rename. Under its own
            // caption, at the left edge like everything else on the page.
            Column {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin

                Label {
                    objectName: "addressCaption"
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeExtraSmall
                    color: Theme.secondaryColor
                    text: qsTr("Address")
                }

                Label {
                    objectName: "addressLabel"
                    width: parent.width
                    wrapMode: Text.WrapAnywhere
                    color: Theme.highlightColor
                    // The relay's string, pinned to plain.
                    textFormat: Text.PlainText
                    text: profile.address
                }
            }

            // Room between the address and the switch: the two are not
            // one thing, and read as one without it.
            Item {
                width: 1
                height: Theme.paddingLarge
            }

            TextSwitch {
                objectName: "readReceiptsSwitch"
                text: qsTr("Send read receipts")
                // The core's own words for mdns_enabled are "should be
                // sent and requested", so this is not one-way: with it
                // off nothing is asked for either, and read marks stop
                // coming back from the people who would have sent them.
                description: qsTr("Both ways: with this off you send none and see none.")
                // Bound to the profile, not held here: the core is what
                // decides, and a switch that drifts from it is a lie. So
                // the tap must not flip it either -- Silica does that by
                // default, which detaches the binding on the first tap and
                // leaves a refused change looking like it took. The tap
                // asks the core for the other state, and the binding
                // shows whatever the core then says.
                automaticCheck: false
                checked: profile.read_receipts
                enabled: profile.loaded
                onClicked: profile.set_read_receipts(!checked)
            }

            SectionHeader {
                text: qsTr("Storage and connectivity")
            }

            // The connection: a dot in the core's colour and the band in
            // words, the row parla's dialog starts its section with.
            Row {
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                spacing: Theme.paddingMedium

                Rectangle {
                    objectName: "connectivityDot"
                    anchors.verticalCenter: parent.verticalCenter
                    width: Theme.paddingLarge
                    height: width
                    radius: width / 2
                    color: page.connectionColor(profile.connectivity)
                }

                Label {
                    objectName: "connectivityLabel"
                    width: parent.width - Theme.paddingLarge - Theme.paddingMedium
                    wrapMode: Text.Wrap
                    color: Theme.highlightColor
                    // A translated literal, chosen by the core's number.
                    textFormat: Text.PlainText
                    text: page.connectionWords(profile.connectivity)
                }
            }

            // The mailbox on the relay: the bar is always there, at
            // nothing until the relay has said how full it is, and the
            // words under it say what the relay said. Drawn here rather
            // than Silica's ProgressBar, whose track sits well inside its
            // own margins: this one runs the width of the text above it.
            Item {
                id: quotaBar
                objectName: "quotaBar"
                /// Percent used, 0 to 100.
                property real value: Math.min(100, profile.quota_percent)
                /// What is said under the bar.
                property string label: page.quotaWords()
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                height: track.height + Theme.paddingSmall + quotaLabel.height

                Rectangle {
                    id: track
                    width: parent.width
                    height: Theme.paddingSmall
                    radius: height / 2
                    color: Theme.rgba(Theme.primaryColor, 0.2)

                    Rectangle {
                        objectName: "quotaFill"
                        width: Math.round(parent.width * quotaBar.value / 100)
                        height: parent.height
                        radius: height / 2
                        color: Theme.highlightColor
                    }
                }

                Label {
                    id: quotaLabel
                    objectName: "quotaLabel"
                    anchors {
                        top: track.bottom
                        topMargin: Theme.paddingSmall
                    }
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pixelSize: Theme.fontSizeSmall
                    color: Theme.secondaryHighlightColor
                    // The relay's own words, when they are what is shown.
                    textFormat: Text.PlainText
                    text: quotaBar.label
                }
            }

            Label {
                objectName: "storageLabel"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                visible: profile.storage_bytes > 0
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryColor
                textFormat: Text.PlainText
                //: How much room the profile takes on the phone. %1 is a size such as "12.3 MB".
                text: qsTr("%1 on this phone").arg(Format.readableSize(profile.storage_bytes))
            }
        }
    }

    Banner {
        objectName: "errorBanner"
        anchors {
            left: parent.left
            right: parent.right
            bottom: notice.top
        }
        text: page.errorMessage
        timeout: 8
        onDismissed: page.errorMessage = ""
    }

    Banner {
        id: notice
        objectName: "notice"
        labelObjectName: "noticeLabel"
        tone: "info"
        timeout: 2
        anchors {
            left: parent.left
            right: parent.right
            bottom: parent.bottom
        }
        onDismissed: notice.text = ""
    }

    Connections {
        target: profile
        // Quiet confirmation that something reached the core, since
        // nothing else on this page says so any more.
        onSaved: notice.show(qsTr("Saved"))
    }
}
