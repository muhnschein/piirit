import QtQuick 2.0
import Sailfish.Silica 1.0
import "../components"

/*
 * Group's or channel's name, description, picture, from its page's pulley. Swipe
 * back sends changed fields, once: each change messages every member. Blank name
 * goes back to current (core refuses empty). Picture changes at once, removal after
 * Silica's countdown.
 */
Page {
    id: page

    /// Group's ChatInfo.
    property QtObject chat

    function save() {
        if (!page.chat || !page.chat.loaded) {
            return
        }
        var name = nameField.text.trim()
        if (page.chat.can_edit && name.length > 0 && name !== page.chat.name) {
            page.chat.rename(name)
        }
        if (page.chat.can_edit_description && descriptionField.text !== page.chat.description) {
            page.chat.set_description(descriptionField.text)
        }
    }

    onStatusChanged: {
        if (status === PageStatus.Deactivating) {
            nameField.focus = false
            descriptionField.focus = false
            page.save()
        }
    }

    // Gallery by URL. Only Attach*Page files name `Sailfish.Pickers`: missing type
    // costs entry, not page.
    function pickPicture() {
        var picker = pageStack.push(Qt.resolvedUrl("AttachPhotoPage.qml"))
        if (picker) {
            picker.picked.connect(function(path) {
                // Core copies file to blob dir; picked file may vanish.
                page.chat.set_picture(path)
            })
        }
    }

    RemorsePopup {
        id: pictureRemorse
        objectName: "pictureRemorse"
    }

    SilicaFlickable {
        anchors.fill: parent
        contentHeight: column.height + Theme.paddingLarge

        PullDownMenu {
            objectName: "pictureMenu"
            visible: page.chat.can_edit

            // Only when there is a picture to remove.
            MenuItem {
                objectName: "removePicture"
                visible: page.chat.avatar_path.length > 0
                text: qsTr("Remove picture")
                //: Silica's countdown before a group's picture is removed.
                onClicked: pictureRemorse.execute(qsTr("Removing picture"), function() {
                    page.chat.clear_picture()
                })
            }
            MenuItem {
                objectName: "changePicture"
                //: Pull-down entry on a group's page: opens the gallery to
                //: pick a new picture for the group.
                text: qsTr("Change picture")
                onClicked: page.pickPicture()
            }
        }

        Column {
            id: column
            width: parent.width
            spacing: Theme.paddingMedium

            PageHeader {
                title: page.chat.is_channel ? qsTr("Edit channel") : qsTr("Edit group")
                description: page.chat.name
            }

            Item {
                width: parent.width
                height: avatar.height + Theme.paddingLarge

                Avatar {
                    id: avatar
                    objectName: "groupAvatar"
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: Theme.itemSizeExtraLarge
                    initial: page.chat.name
                    ownColor: page.chat.color
                    picturePath: page.chat.avatar_path
                }
            }

            TextField {
                id: nameField
                objectName: "groupNameField"
                visible: page.chat.can_edit
                width: parent.width
                placeholderText: nameField.label
                label: page.chat.is_channel ? qsTr("Channel name") : qsTr("Group name")
                Component.onCompleted: nameField.text = page.chat.name
            }

            // Keeps line breaks.
            TextArea {
                id: descriptionField
                objectName: "descriptionField"
                visible: page.chat.can_edit_description
                width: parent.width
                //: Label of the text that says what a group or channel is
                //: for, on its info page, and the prompt in its empty field.
                label: qsTr("Description")
                placeholderText: label
                Component.onCompleted: descriptionField.text = page.chat.description
            }

            Label {
                objectName: "saveHint"
                x: Theme.horizontalPageMargin
                width: parent.width - 2 * Theme.horizontalPageMargin
                wrapMode: Text.Wrap
                font.pixelSize: Theme.fontSizeSmall
                color: Theme.secondaryHighlightColor
                text: page.chat.is_channel
                      ? qsTr("Everyone in the channel sees these changes. Swipe back to save.")
                      : qsTr("Everyone in the group sees these changes. Swipe back to save.")
            }
        }

        VerticalScrollDecorator {}
    }
}
