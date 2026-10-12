pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.services
import qs.modules.common
import qs.modules.common.functions
import qs.modules.iris.components
import qs.modules.iris.style

IrisSidebarSectionFrame {
    id: notifications
    readonly property var latest: Notifications.list.length > 0 ? Notifications.list[Notifications.list.length - 1] : null
    property var unfoldedApps: ({})
    title: Translation.tr("Notifications")
    glyph: "notifications"
    tint: IrisStyle.identity.pink
    detail: Notifications.list.length > 0 ? String(Notifications.list.length) : ""
    actions: [
        IrisIconButton {
            materialIcon: Notifications.manualDndActive ? "notifications_off" : "notifications_active"
            selected: Notifications.manualDndActive
            Accessible.name: Translation.tr("Do not disturb")
            onClicked: Notifications.toggleSilent()
        },
        IrisIconButton {
            visible: Notifications.list.length > 0
            materialIcon: "clear_all"
            Accessible.name: Translation.tr("Clear all")
            onClicked: Notifications.discardAllNotifications()
        }
    ]
    summary: [
        EmptyLabel { visible: notifications.latest === null; text: Translation.tr("You're all caught up.") },
        RowLayout {
            Layout.fillWidth: true
            visible: notifications.latest !== null
            spacing: 8 * notifications.sectionHost.d
            IrisNotificationIcon {
                size: Math.round(22 * notifications.sectionHost.d)
                showImage: false
                appName: String(notifications.latest?.appName ?? "")
                appIcon: String(notifications.latest?.appIcon ?? "")
                summary: String(notifications.latest?.summary ?? "")
            }
            IrisText {
                Layout.fillWidth: true
                text: String(notifications.latest?.summary ?? "")
                textFormat: Text.PlainText
                font.weight: IrisStyle.weight(Font.Medium)
                font.pixelSize: IrisStyle.typeLabel
                elide: Text.ElideRight
            }
        }
    ]
    EmptyLabel { visible: Notifications.list.length === 0; text: Translation.tr("You're all caught up.") }
    Repeater {
        model: Notifications.appNameList
        NoticeStack {
            required property string modelData
            Layout.fillWidth: true
            appName: modelData
            unfolded: notifications.unfoldedApps[modelData] === true
            onToggle: {
                const next = Object.assign({}, notifications.unfoldedApps)
                next[modelData] = !next[modelData]
                notifications.unfoldedApps = next
            }
        }
    }

    component NoticeCard: Rectangle {
        id: notice
        required property var notification
        property int hidden: 0
        signal unfold()
        implicitHeight: noticeBody.implicitHeight + 20 * notifications.sectionHost.d
        radius: IrisStyle.radiusRow
        color: IrisStyle.surfaceHighest

        TapHandler {
            enabled: notice.hidden > 0
            onTapped: notice.unfold()
        }
        HoverHandler {
            enabled: notice.hidden > 0
            cursorShape: Qt.PointingHandCursor
        }

        ColumnLayout {
            id: noticeBody
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 10 * notifications.sectionHost.d
            spacing: 3 * notifications.sectionHost.d
            RowLayout {
                Layout.fillWidth: true
                spacing: 8 * notifications.sectionHost.d
                IrisNotificationIcon {
                    size: Math.round(22 * notifications.sectionHost.d)
                    showImage: false
                    appName: String(notice.notification?.appName ?? "")
                    appIcon: String(notice.notification?.appIcon ?? "")
                    summary: String(notice.notification?.summary ?? "")
                    critical: String(notice.notification?.urgency ?? "") === "critical"
                }
                IrisText { Layout.fillWidth: true; text: String(notice.notification?.appName ?? ""); role: IrisText.Meta; elide: Text.ElideRight }
                Rectangle {
                    visible: notice.hidden > 0
                    implicitHeight: Math.round(20 * notifications.sectionHost.d)
                    implicitWidth: Math.max(implicitHeight, moreLabel.implicitWidth + Math.round(12 * notifications.sectionHost.d))
                    radius: height / 2
                    color: IrisStyle.fillHover
                    IrisText {
                        id: moreLabel
                        anchors.centerIn: parent
                        text: "+" + notice.hidden
                        font.family: IrisStyle.fontNumbers
                        font.pixelSize: IrisStyle.typeMeta
                        font.weight: IrisStyle.weight(Font.DemiBold)
                    }
                }
                IrisIconButton {
                    materialIcon: "close"
                    iconSize: Math.round(15 * notifications.sectionHost.d)
                    Accessible.name: Translation.tr("Dismiss notification")
                    onClicked: Notifications.discardNotification(notice.notification.notificationId)
                }
            }
            IrisText { Layout.fillWidth: true; text: String(notice.notification?.summary ?? ""); textFormat: Text.PlainText; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight; font.weight: IrisStyle.weight(Font.DemiBold) }
            IrisText { Layout.fillWidth: true; visible: text.length > 0; text: String(notice.notification?.body ?? ""); textFormat: Text.PlainText; wrapMode: Text.Wrap; maximumLineCount: 4; elide: Text.ElideRight; role: IrisText.Meta }
            Flow {
                Layout.fillWidth: true
                visible: (notice.notification?.actions ?? []).length > 0
                spacing: 4 * notifications.sectionHost.d
                Repeater {
                    model: notice.notification?.actions ?? []
                    IrisButton {
                        required property var modelData
                        text: modelData.identifier === "default" ? Translation.tr("Open") : modelData.text
                        implicitHeight: 30 * notifications.sectionHost.d
                        onClicked: { Notifications.attemptInvokeAction(notice.notification.notificationId, modelData.identifier); notifications.sectionHost.navigate() }
                    }
                }
            }
        }
    }

    component NoticeStack: ColumnLayout {
        id: stack
        required property string appName
        property bool unfolded: false
        signal toggle()
        readonly property var items: (Notifications.groupsByAppName[stack.appName]?.notifications ?? []).slice().reverse()
        readonly property int count: stack.items.length
        readonly property bool folded: stack.count > 1 && !stack.unfolded
        spacing: 6 * notifications.sectionHost.d

        RowLayout {
            visible: stack.count > 1 && stack.unfolded
            Layout.fillWidth: true
            Layout.leftMargin: 4 * notifications.sectionHost.d
            spacing: 4 * notifications.sectionHost.d
            IrisText { Layout.fillWidth: true; text: stack.appName; font.weight: IrisStyle.weight(Font.DemiBold); font.pixelSize: IrisStyle.typeLabel; elide: Text.ElideRight }
            IrisButton {
                text: Translation.tr("Show less")
                quiet: true
                implicitHeight: 26 * notifications.sectionHost.d
                onClicked: stack.toggle()
            }
            IrisIconButton {
                materialIcon: "clear_all"
                iconSize: Math.round(16 * notifications.sectionHost.d)
                Accessible.name: Translation.tr("Clear %1").arg(stack.appName)
                onClicked: Notifications.discardNotificationsForApp(stack.appName)
            }
        }

        Repeater {
            model: stack.folded ? stack.items.slice(0, 1) : stack.items
            NoticeCard {
                required property var modelData
                Layout.fillWidth: true
                notification: modelData
                hidden: stack.folded ? stack.count - 1 : 0
                onUnfold: stack.toggle()
            }
        }

        // Only what shows below the card is drawn: under a translucent material a plate tucked behind
        // the card reads through it and the stack looks mixed into the card.
        Item {
            visible: stack.folded
            clip: true
            Layout.fillWidth: true
            Layout.topMargin: -stack.spacing
            implicitHeight: Math.round((stack.count > 2 ? 12 : 6) * notifications.sectionHost.d)
            Rectangle {
                visible: stack.count > 2
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                width: parent.width - Math.round(28 * notifications.sectionHost.d)
                height: IrisStyle.radiusRow * 2
                radius: IrisStyle.radiusRow
                color: ColorUtils.mix(IrisStyle.surfaceHighest, IrisStyle.surfaceHigh, 0.7)
            }
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: stack.count > 2 ? Math.round(6 * notifications.sectionHost.d) : 0
                width: parent.width - Math.round(14 * notifications.sectionHost.d)
                height: IrisStyle.radiusRow * 2
                radius: IrisStyle.radiusRow
                color: ColorUtils.mix(IrisStyle.surfaceHighest, IrisStyle.surfaceHigh, 0.4)
            }
            TapHandler { onTapped: stack.toggle() }
            HoverHandler { cursorShape: Qt.PointingHandCursor }
        }
    }

    component EmptyLabel: IrisText {
        Layout.fillWidth: true
        color: IrisStyle.muted
        wrapMode: Text.WordWrap
        font.pixelSize: IrisStyle.typeMeta
    }
}
