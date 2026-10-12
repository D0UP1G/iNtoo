pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Shapes
import qs.modules.iris.style

Shape {
    id: gaugeRing
    property real progress: 0
    property color tint: IrisStyle.accent
    property real density: IrisStyle.density
    property real stroke: Math.round(5 * gaugeRing.density)
    property real start: -90
    preferredRendererType: Shape.CurveRenderer
    ShapePath {
        strokeColor: IrisStyle.tintFill(gaugeRing.tint)
        strokeWidth: gaugeRing.stroke
        fillColor: "transparent"
        PathAngleArc { centerX: gaugeRing.width / 2; centerY: gaugeRing.height / 2; radiusX: gaugeRing.width / 2 - gaugeRing.stroke / 2; radiusY: gaugeRing.width / 2 - gaugeRing.stroke / 2; startAngle: 0; sweepAngle: 360 }
    }
    ShapePath {
        strokeColor: gaugeRing.tint
        strokeWidth: gaugeRing.stroke
        fillColor: "transparent"
        capStyle: ShapePath.RoundCap
        PathAngleArc { centerX: gaugeRing.width / 2; centerY: gaugeRing.height / 2; radiusX: gaugeRing.width / 2 - gaugeRing.stroke / 2; radiusY: gaugeRing.width / 2 - gaugeRing.stroke / 2; startAngle: gaugeRing.start; sweepAngle: 360 * Math.max(0, Math.min(1, gaugeRing.progress)) }
    }
}
