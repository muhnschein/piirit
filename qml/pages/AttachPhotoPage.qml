import QtQuick 2.0
import Sailfish.Pickers 1.0

/*
 * Gallery picker. Own page: `Sailfish.Pickers` types resolve at load, so a
 * missing type costs one button, not the conversation.
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
