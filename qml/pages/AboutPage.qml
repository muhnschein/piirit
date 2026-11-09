import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Versions, licence, source; what a bug report needs and a GPL app shows. From
 * SettingsPage. Core version asked on open (`check_health`); blank till answered.
 */
Page {
    id: page

    /// Unused: every settings page takes it (SettingsPage pushes it).
    property int accountId

    readonly property string sourceUrl: "https://github.com/muhnschein/piirit"

    //: %1 is the version number, such as 2.2.0.
    readonly property string appLine: qsTr("Piirit %1").arg(core.app_version)
    //: %1 is the version number of the Delta Chat core, such as 2.68.0.
    readonly property string coreLine: core.core_version.length > 0
                                       ? qsTr("Delta Chat core %1").arg(core.core_version) : ""

    /// Versions on clipboard, then the issue tracker.
    function reportProblem() {
        Clipboard.text = page.appLine + "\n" + page.coreLine
        Qt.openUrlExternally(page.sourceUrl + "/issues")
    }

    Component.onCompleted: {
        if (core.status === "ready") {
            core.check_health()
        }
    }

    // Opened before core up: ask once it is.
    Connections {
        target: core
        onStatus_changed: {
            if (core.status === "ready") {
                core.check_health()
            }
        }
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: qsTr("About")
            }

            Image {
                anchors.horizontalCenter: parent.horizontalCenter
                width: Theme.itemSizeHuge
                height: width
                sourceSize.width: width
                sourceSize.height: height
                source: "/usr/share/icons/hicolor/172x172/apps/harbour-piirit.png"
            }

            Label {
                objectName: "appVersion"
                anchors.horizontalCenter: parent.horizontalCenter
                font.pixelSize: Theme.fontSizeLarge
                text: page.appLine
            }

            Label {
                objectName: "coreVersion"
                anchors.horizontalCenter: parent.horizontalCenter
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.highlightColor
                text: page.coreLine
            }

            Item {
                width: 1
                height: Theme.paddingLarge
            }

            ValueButton {
                objectName: "sourceEntry"
                width: parent.width
                label: qsTr("Source code and licence")
                //: Under "Source code and licence". GPL-3.0 is the licence's short name.
                description: qsTr("Piirit is free software under the GPL-3.0.")
                onClicked: Qt.openUrlExternally(page.sourceUrl)
            }

            ValueButton {
                objectName: "reportEntry"
                width: parent.width
                label: qsTr("Report a problem")
                description: qsTr("The versions are copied for your report.")
                onClicked: page.reportProblem()
            }

            ValueButton {
                objectName: "introEntry"
                width: parent.width
                //: Opens the intro slides that explain what Delta Chat is.
                label: qsTr("About Delta Chat")
                onClicked: pageStack.push(Qt.resolvedUrl("IntroPage.qml"), { tour: true })
            }
        }

        VerticalScrollDecorator {}
    }
}
