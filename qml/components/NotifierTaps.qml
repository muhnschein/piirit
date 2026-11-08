import QtQuick 2.0
import Nemo.DBus 2.0

/*
 * Notification taps land here (Notifier.qml's remote action names it). Sailjail lets app own
 * OrganizationName.ApplicationName from desktop file, `piirit.piirit`. One per app, window's:
 * second adaptor at "/" fails, its teardown unregisters "/" and name (CallCenter's taps too).
 */
DBusAdaptor {
    id: taps

    /// Tapped: chat to open.
    signal chatRequested(int chatId)

    service: "piirit.piirit"
    path: "/"
    iface: "piirit.piirit"
    xml: "  <interface name=\"piirit.piirit\">\n" +
         "    <method name=\"showChat\">\n" +
         "      <arg name=\"chatId\" type=\"i\" direction=\"in\"/>\n" +
         "    </method>\n" +
         "  </interface>\n"

    /// Tap's call. Chat id is notification's own argument: survives app restart.
    function showChat(chatId) {
        taps.chatRequested(chatId)
    }
}
