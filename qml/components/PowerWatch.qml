import QtQuick 2.0
import Nemo.DBus 2.0

/*
 * Charger and battery saving mode, from MCE on system bus (sailjail `Base` grants
 * `com.nokia.mce`). Asked once per MCE appearance, then signals only: no polling.
 * Names: mce-dev `dbus-names.h`, `mode-names.h`.
 */
Item {
    id: watch

    /// Charger in (`charger_state` "on"). False until MCE answers.
    property bool charging: false
    /// Battery saving mode on (`psm_state`). False until MCE answers.
    property bool saving: false

    /// `DBusInterface.Available` by value; see NetworkWatch.
    readonly property int dbusAvailable: 2

    /// MCE charger state string in.
    function heardCharger(state) {
        watch.charging = ("" + state) === "on"
    }

    /// MCE battery saving flag in.
    function heardSaving(active) {
        watch.saving = active === true
    }

    /// Ask MCE now. Failure: state kept (false: automatic jobs wait for charger).
    function look() {
        request.typedCall("get_charger_state", [], function (state) {
            watch.heardCharger(state)
        }, function () {})
        request.typedCall("get_psm_state", [], function (active) {
            watch.heardSaving(active)
        }, function () {})
    }

    DBusInterface {
        id: request
        objectName: "mceRequest"
        bus: DBus.SystemBus
        service: "com.nokia.mce"
        path: "/com/nokia/mce/request"
        iface: "com.nokia.mce.request"
        // MCE restart: ask again.
        watchServiceStatus: true
        onStatusChanged: {
            if (request.status === watch.dbusAvailable) {
                watch.look()
            }
        }
    }

    DBusInterface {
        objectName: "mceSignal"
        bus: DBus.SystemBus
        service: "com.nokia.mce"
        path: "/com/nokia/mce/signal"
        iface: "com.nokia.mce.signal"
        signalsEnabled: true

        // Nemo.DBus calls function named as signal.
        function charger_state_ind(state) {
            watch.heardCharger(state)
        }

        function psm_state_ind(active) {
            watch.heardSaving(active)
        }
    }

    Component.onCompleted: watch.look()
}
