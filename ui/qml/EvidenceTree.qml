pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root

    property var theme
    property var controller
    property var evidenceModel
    color: root.theme.background

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: root.theme.space2
        spacing: root.theme.space2

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: root.theme.controlHeight
            spacing: root.theme.space2

            Text {
                text: "Evidence"
                color: root.theme.text
                font.family: root.theme.uiFont
                font.pixelSize: 14
                font.weight: Font.DemiBold
                Layout.fillWidth: true
            }

            Button {
                id: addButton
                text: "Add image"
                onClicked: root.controller.addImage()
                Accessible.name: "Add disk image"
                Layout.preferredHeight: root.theme.controlHeight
                padding: root.theme.space2
                background: Rectangle {
                    radius: root.theme.radius
                    color: addButton.hovered ? Qt.darker(root.theme.accent, 1.12) : root.theme.accent
                    border.color: root.theme.accent
                    border.width: 1
                }
                contentItem: Text {
                    text: addButton.text
                    color: root.theme.accentContrast
                    font.family: root.theme.uiFont
                    font.pixelSize: 12
                    font.weight: Font.DemiBold
                }
            }
        }

        Text {
            visible: root.evidenceModel.evidenceCount === 0
            Layout.fillWidth: true
            text: "No evidence added. Add a disk image to begin."
            color: root.theme.textMuted
            font.family: root.theme.uiFont
            font.pixelSize: 12
            wrapMode: Text.WordWrap
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.evidenceModel
            focus: true
            activeFocusOnTab: true
            Accessible.name: "Evidence tree"
            Accessible.description: "Disk images, partitions, and filesystem nodes"

            delegate: ItemDelegate {
                id: row
                required property int index
                required property string name
                required property string kind
                required property int depth
                required property bool expandable
                required property bool expanded
                width: ListView.view.width
                height: root.theme.rowHeight
                padding: 0
                hoverEnabled: true
                Accessible.name: name
                Accessible.description: kind

                background: Rectangle {
                    color: ListView.isCurrentItem ? root.theme.raised : "transparent"
                    border.width: ListView.isCurrentItem ? 1 : 0
                    border.color: root.theme.accent
                }

                contentItem: RowLayout {
                    spacing: root.theme.space1
                    anchors.left: parent.left
                    anchors.leftMargin: root.theme.space2 + row.depth * root.theme.space3
                    anchors.right: parent.right
                    anchors.rightMargin: root.theme.space2

                    Text {
                        text: row.expandable ? (row.expanded ? "▾" : "▸") : " "
                        color: root.theme.textMuted
                        font.family: root.theme.uiFont
                        font.pixelSize: 12
                        Layout.preferredWidth: 14
                        horizontalAlignment: Text.AlignHCenter
                    }

                    Text {
                        text: row.name
                        color: root.theme.text
                        font.family: root.theme.uiFont
                        font.pixelSize: 12
                        elide: Text.ElideRight
                        Layout.fillWidth: true
                    }

                    Text {
                        text: row.kind
                        color: root.theme.textMuted
                        font.family: root.theme.uiFont
                        font.pixelSize: 10
                        elide: Text.ElideRight
                        Layout.maximumWidth: 92
                    }
                }

                onClicked: {
                    list.currentIndex = index;
                    if (expandable) {
                        root.evidenceModel.toggle(index);
                    } else {
                        root.controller.activateNode(root.evidenceModel.nodeAt(index));
                    }
                }

                onDoubleClicked: root.controller.activateNode(root.evidenceModel.nodeAt(index))
            }
        }

        Text {
            Layout.fillWidth: true
            text: "Select an image, partition, or filesystem node. Double-click a partition to browse."
            color: root.theme.textMuted
            font.family: root.theme.uiFont
            font.pixelSize: 10
            wrapMode: Text.WordWrap
        }
    }
}
