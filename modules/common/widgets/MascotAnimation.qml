import QtQuick

// Inert compatibility shim: the upstream mascot artwork is not redistributed.
AnimatedImage {
    property string pose: ""
    property string surface: ""
    property string fallbackSurface: ""
    readonly property bool active: false

    visible: false
    playing: false
    source: ""
}
