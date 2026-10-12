pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.services
import qs.modules.common.widgets
import qs.modules.iris.style
import qs.modules.iris.components

ColumnLayout {
    id: root
    readonly property real d: IrisStyle.density
            spacing: 10 * root.d
            CardHeader {
                glyph: Network.ethernet ? "lan" : Network.wifiEnabled ? "wifi" : "wifi_off"
                tint: IrisStyle.identity.blue
                title: Translation.tr("Network")
                detail: Network.ethernet ? Translation.tr("Ethernet")
                    : !Network.wifiEnabled ? Translation.tr("Wi-Fi off")
                    : Network.networkName.length > 0 ? Network.networkName : Translation.tr("Not connected")
                IrisIconButton {
                    materialIcon: "refresh"
                    enabled: Network.wifiEnabled
                    Accessible.name: Translation.tr("Scan for networks")
                    onClicked: Network.rescanWifi()
                }
                IrisIconButton {
                    materialIcon: Network.wifiEnabled ? "wifi" : "wifi_off"
                    selected: Network.wifiEnabled
                    Accessible.name: Translation.tr("Wi-Fi")
                    onClicked: Network.toggleWifi()
                }
            }
            IrisNetworkList {
                Layout.fillWidth: true
                Layout.leftMargin: -6 * root.d
                Layout.rightMargin: -6 * root.d
            }
        }
