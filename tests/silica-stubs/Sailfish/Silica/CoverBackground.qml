import QtQuick 2.0

// Stands in for Silica's CoverBackground. `status` is the cover's own,
// saying whether the cover is the thing being looked at. It starts at
// Cover.Active, because a cover under test is one being read; a test that
// is about what happens while nobody is looking sets it itself.
Item {
    property int status: 2
}
