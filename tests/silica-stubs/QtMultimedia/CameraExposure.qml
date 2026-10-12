import QtQuick 2.0

// The camera's exposure, a type of its own so a page's grouped
// `exposure.exposureCompensation` binding resolves against a declared
// property, and a test reads back what the page asked for.
QtObject {
    property real exposureCompensation: 0
}
