import QtQuick

QtObject {
    id: root

    property bool dark: false

    readonly property color background: dark ? "#141311" : "#F4F1EA"
    readonly property color surface: dark ? "#1D1B18" : "#FBFAF6"
    readonly property color raised: dark ? "#26231F" : "#FFFFFF"
    readonly property color text: dark ? "#F3EEE4" : "#1C1B18"
    readonly property color textMuted: dark ? "#A8A294" : "#6F6A5E"
    readonly property color border: dark ? "#6F6A5E" : "#8A8578"
    readonly property color borderSubtle: dark ? "#3A362F" : "#C9C2B4"
    readonly property color accent: dark ? "#E0A458" : "#8A4B00"
    readonly property color accentContrast: dark ? "#1A1712" : "#FFFFFF"
    readonly property color error: dark ? "#FF9B8A" : "#A33A2A"
    readonly property color success: dark ? "#9FD3A8" : "#2F5D3A"

    readonly property int radius: 4
    readonly property int rowHeight: 30
    readonly property int controlHeight: 32
    readonly property int space1: 4
    readonly property int space2: 8
    readonly property int space3: 12
    readonly property int space4: 16
    readonly property int space5: 24

    readonly property string uiFont: "Noto Sans"
    readonly property string monoFont: "Noto Sans Mono"
}
