import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root

    property var theme
    property string title: ""
    property string body: ""
    property string actionText: ""
    signal actionTriggered()

    ColumnLayout {
        anchors.centerIn: parent
        width: Math.min(parent.width - root.theme.space5 * 2, 420)
        spacing: root.theme.space3

        Text {
            Layout.fillWidth: true
            text: root.title
            color: root.theme.text
            font.family: root.theme.uiFont
            font.pixelSize: 16
            font.weight: Font.DemiBold
            wrapMode: Text.WordWrap
        }

        Text {
            Layout.fillWidth: true
            text: root.body
            color: root.theme.textMuted
            font.family: root.theme.uiFont
            font.pixelSize: 13
            wrapMode: Text.WordWrap
            lineHeight: 1.35
        }

        Button {
            id: actionButton
            visible: root.actionText.length > 0
            text: root.actionText
            onClicked: root.actionTriggered()
            Accessible.name: root.actionText
            Accessible.description: root.body
            Layout.preferredHeight: root.theme.controlHeight
            padding: root.theme.space3
            background: Rectangle {
                radius: root.theme.radius
                color: actionButton.hovered ? Qt.darker(root.theme.accent, 1.12) : root.theme.accent
                border.color: root.theme.accent
                border.width: 1
            }
            contentItem: Text {
                text: actionButton.text
                color: root.theme.accentContrast
                font.family: root.theme.uiFont
                font.pixelSize: 13
                font.weight: Font.DemiBold
                horizontalAlignment: Text.AlignHCenter
                verticalAlignment: Text.AlignVCenter
            }
        }
    }
}
