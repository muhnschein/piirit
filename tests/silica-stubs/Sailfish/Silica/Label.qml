import QtQuick 2.0
import Sailfish.Silica 1.0

// Silica fades or elides Label not fitting width; Qt Text paints over edge.
// Modelled so test sees width binding never resolved (binding loop, result off
// screen).
Text {
    property int truncationMode: TruncationMode.None
    // Silica's default; Qt's is black.
    color: Theme.primaryColor
    elide: truncationMode === TruncationMode.None ? Text.ElideNone : Text.ElideRight
}
