pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import qs.services
import qs.modules.common
import qs.modules.common.functions
import qs.modules.common.widgets
import qs.modules.iris.components
import qs.modules.iris.style

IrisSidebarSectionFrame {
    id: weather

    component EmptyLabel: IrisText {
        Layout.fillWidth: true
        color: IrisStyle.muted
        wrapMode: Text.WordWrap
        font.pixelSize: IrisStyle.typeMeta
    }

    readonly property bool hasData: Weather.data.temp !== "--°C" && Weather.data.temp !== "--°F"
    readonly property bool night: Weather.isNightNow()
    readonly property string condition: weather.hasData
        ? (Icons.getWeatherIcon(Weather.data.wCode, weather.night) ?? "") : ""
    glyph: weather.condition.length > 0 ? weather.condition : weather.night ? "bedtime" : "partly_cloudy_day"
    tint: weather.condition.length > 0 ? IrisStyle.skyLight(weather.condition)
        : weather.night ? IrisStyle.identity.indigo : IrisStyle.identity.blue
    ink: IrisStyle.text
    title: Weather.showVisibleCity && Weather.visibleCity ? Weather.visibleCity : Translation.tr("Weather")
    detail: !weather.open && weather.hasData ? Weather.data.temp : ""
    actions: IrisIconButton {
        materialIcon: "refresh"
        enabled: Weather.enabled && !Weather.hasRunningRequests()
        Accessible.name: Translation.tr("Refresh weather")
        onClicked: Weather.forceRefresh()
    }
    summary: IrisText {
        Layout.fillWidth: true
        text: !Weather.enabled ? Translation.tr("Weather is off")
            : weather.hasData ? Weather.data.description + "  ·  " + Translation.tr("Feels like %1").arg(Weather.data.tempFeelsLike)
            : Translation.tr("Fetching your forecast…")
        color: IrisStyle.strongOf(weather.ink)
        font.pixelSize: IrisStyle.typeLabel
        elide: Text.ElideRight
    }
    RowLayout {
        visible: Weather.enabled && weather.hasData
        Layout.fillWidth: true
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 0
            IrisText { text: Weather.data.temp; color: IrisStyle.text; font.family: IrisStyle.fontNumbers; font.pixelSize: 44 * IrisStyle.typeScale; font.weight: IrisStyle.weight(Font.Light) }
            IrisText { Layout.fillWidth: true; text: Weather.data.description; color: IrisStyle.textSecondary; font.pixelSize: IrisStyle.typeLabel; wrapMode: Text.WordWrap }
        }
        MaterialSymbol { text: Icons.getWeatherIcon(Weather.data.wCode, Weather.isNightNow()) ?? "cloud"; fill: 1; iconSize: 46 * weather.sectionHost.d; color: weather.night ? IrisStyle.identity.lavender : IrisStyle.secondaryAccent }
    }
    RowLayout {
        visible: Weather.enabled && weather.hasData
        Layout.fillWidth: true
        IrisText { text: Translation.tr("Feels like %1").arg(Weather.data.tempFeelsLike); color: IrisStyle.textSecondary; font.pixelSize: IrisStyle.typeMeta }
        Item { Layout.fillWidth: true }
        MaterialSymbol { text: "humidity_percentage"; fill: 1; iconSize: 14 * weather.sectionHost.d; color: IrisStyle.textTertiary }
        IrisText { text: Weather.data.humidity; color: IrisStyle.textSecondary; font.pixelSize: IrisStyle.typeMeta }
    }
    EmptyLabel { visible: !Weather.enabled; text: Translation.tr("Enable weather in Settings to see your forecast.") }
    EmptyLabel {
        visible: Weather.enabled && !weather.hasData
        text: Weather.hasRunningRequests() ? Translation.tr("Fetching your forecast…") : Translation.tr("Weather unavailable. Try refreshing.")
    }
}
