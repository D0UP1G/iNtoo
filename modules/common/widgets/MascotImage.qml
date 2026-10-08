import QtQuick

// Inert compatibility shim: the upstream mascot artwork is not redistributed.
Image {
    property string pose: ""
    property bool previewMode: false
    property bool playAnimation: true
    property string surface: ""
    property string fallbackSurface: ""
    readonly property bool active: false

    visible: false
    source: ""
}
