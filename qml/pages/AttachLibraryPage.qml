import QtQuick 2.0
import Sailfish.Pickers 1.0

/*
 * Library: everything phone indexed, for attaching. Platform picker over
 * pictures, videos, music, documents at once.
 *
 * Own page, pushed by URL: `Sailfish.Pickers` types resolve at load; a
 * missing type costs one button, not the conversation.
 */
ContentPickerPage {
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
