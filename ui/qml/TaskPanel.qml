pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root

    property var theme
    property var controller
    property var taskModel
    color: root.theme.background

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: root.theme.space2
        spacing: root.theme.space2

        Text {
            text: "Background tasks"
            color: root.theme.text
            font.family: root.theme.uiFont
            font.pixelSize: 14
            font.weight: Font.DemiBold
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.taskModel
            focus: true
            activeFocusOnTab: true
            Accessible.name: "Background tasks"

            delegate: Rectangle {
                id: taskRow
                required property string jobId
                required property string title
                required property string status
                required property string detail
                required property real progress
                required property bool cancellable
                width: ListView.view.width
                height: 58
                color: root.theme.surface
                border.color: root.theme.borderSubtle
                border.width: 1
                radius: root.theme.radius

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: root.theme.space2
                    spacing: root.theme.space2

                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Text {
                            text: taskRow.title
                            color: root.theme.text
                            font.family: root.theme.uiFont
                            font.pixelSize: 12
                            font.weight: Font.Medium
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                        Text {
                            text: taskRow.status + (taskRow.detail.length > 0 ? ": " + taskRow.detail : "")
                            color: taskRow.status === "failed" ? root.theme.error
                                   : taskRow.status === "partial" ? root.theme.accent
                                   : root.theme.textMuted
                            font.family: root.theme.uiFont
                            font.pixelSize: 10
                            elide: Text.ElideRight
                            Layout.fillWidth: true
                        }
                    }

                    Rectangle {
                        Layout.preferredWidth: 90
                        Layout.preferredHeight: 4
                        radius: 2
                        color: root.theme.borderSubtle
                        Rectangle {
                            width: parent.width * taskRow.progress
                            height: parent.height
                            radius: 2
                            color: root.theme.accent
                        }
                    }

                    Button {
                        id: cancelButton
                        visible: taskRow.cancellable
                        text: "Cancel"
                        onClicked: root.controller.cancelJob(taskRow.jobId)
                        Accessible.name: "Cancel task " + taskRow.title
                        Layout.preferredHeight: root.theme.controlHeight
                        padding: root.theme.space2
                        background: Rectangle {
                            radius: root.theme.radius
                            color: cancelButton.hovered ? root.theme.raised : "transparent"
                            border.color: root.theme.border
                            border.width: 1
                        }
                        contentItem: Text {
                            text: cancelButton.text
                            color: root.theme.text
                            font.family: root.theme.uiFont
                            font.pixelSize: 11
                        }
                    }
                }
            }
        }

        Text {
            visible: root.taskModel.rowCount() === 0
            Layout.fillWidth: true
            text: "No background tasks. Acquisition, verification, analysis, and extraction jobs appear here."
            color: root.theme.textMuted
            font.family: root.theme.uiFont
            font.pixelSize: 11
            wrapMode: Text.WordWrap
        }
    }
}
