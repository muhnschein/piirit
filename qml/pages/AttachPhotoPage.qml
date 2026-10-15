import QtQuick 2.0
import Sailfish.Pickers 1.0

/*
 * Gallery picker. Own page: missing `Sailfish.Pickers` types cost one button, not the chat.
 */
ImagePickerPage {
    id: picker

    /// Absolute path of chosen file.
    signal picked(string path)

    onSelectedContentPropertiesChanged: {
        // Core copies file, so picked one may go away.
        if (selectedContentProperties.filePath) {
            picker.picked(selectedContentProperties.filePath)
        }
    }
}
