import QtQuick 2.0

// Silica enum via QML `enum`: host Qt 5.15 supports it, device Qt 5.6 never
// sees stubs.
QtObject {
    enum Status { Inactive, Activating, Active, Deactivating }
}
