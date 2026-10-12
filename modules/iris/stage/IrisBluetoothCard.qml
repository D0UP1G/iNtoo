pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import Quickshell.Bluetooth
import qs.services
import qs.modules.common.widgets
import qs.modules.iris.style
import qs.modules.iris.components

ColumnLayout {
    id: root
    readonly property real d: IrisStyle.density
            spacing: 10 * root.d
            CardHeader {
                glyph: BluetoothStatus.enabled ? "bluetooth" : "bluetooth_disabled"
                tint: IrisStyle.identity.sky
                title: Translation.tr("Bluetooth")
                detail: BluetoothStatus.activeDeviceSummary() || (BluetoothStatus.enabled
                    ? Translation.tr("Nothing connected") : Translation.tr("Off"))
                IrisIconButton {
                    materialIcon: BluetoothStatus.enabled ? "bluetooth" : "bluetooth_disabled"
                    selected: BluetoothStatus.enabled
                    enabled: BluetoothStatus.available
                    Accessible.name: Translation.tr("Bluetooth")
                    onClicked: if (Bluetooth.defaultAdapter) Bluetooth.defaultAdapter.enabled = !Bluetooth.defaultAdapter.enabled
                }
            }
            IrisBluetoothList {
                Layout.fillWidth: true
                Layout.leftMargin: -6 * root.d
                Layout.rightMargin: -6 * root.d
            }
        }
