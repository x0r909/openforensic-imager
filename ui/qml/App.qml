pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: window
    color: theme.background

    property var app
    property var evidenceModel
    property var fileModel
    property var propertiesModel
    property var taskModel
    property var viewerModel

    Theme {
        id: theme
        dark: window.app ? window.app.dark : false
    }

    property var selectedNode: ({})

    Connections {
        target: window.app
        function onErrorRaised(message) {
            errorBanner.text = message;
            errorBanner.visible = true;
            errorTimer.restart();
        }
    }

    Timer {
        id: errorTimer
        interval: 8000
        onTriggered: errorBanner.visible = false
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Rectangle {
            id: errorBanner
            visible: false
            property string text: ""
            Layout.fillWidth: true
            Layout.preferredHeight: visible ? 38 : 0
            color: theme.surface
            border.color: theme.error
            border.width: 1
            Text {
                anchors.fill: parent
                anchors.margins: theme.space2
                text: errorBanner.text
                color: theme.error
                font.family: theme.uiFont
                font.pixelSize: 12
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
                Accessible.name: "Error: " + errorBanner.text
            }
        }

        SplitView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            orientation: Qt.Horizontal

            Rectangle {
                SplitView.preferredWidth: 320
                SplitView.minimumWidth: 240
                color: theme.background
                border.color: theme.borderSubtle
                border.width: 1

                EvidenceTree {
                    anchors.fill: parent
                    theme: theme
                    controller: window.app
                    evidenceModel: window.evidenceModel
                }
            }

            SplitView {
                SplitView.fillWidth: true
                orientation: Qt.Vertical

                SplitView {
                    SplitView.fillHeight: true
                    SplitView.minimumHeight: 260
                    orientation: Qt.Horizontal

                    Rectangle {
                        SplitView.fillWidth: true
                        SplitView.minimumWidth: 380
                        color: theme.background
                        border.color: theme.borderSubtle
                        border.width: 1

                        FileList {
                            anchors.fill: parent
                            theme: theme
                            controller: window.app
                            fileModel: window.fileModel
                        }
                    }

                    Rectangle {
                        SplitView.preferredWidth: 300
                        SplitView.minimumWidth: 240
                        color: theme.background
                        border.color: theme.borderSubtle
                        border.width: 1

                        PropertiesPanel {
                            anchors.fill: parent
                            theme: theme
                            propertiesModel: window.propertiesModel
                        }
                    }
                }

                SplitView {
                    SplitView.preferredHeight: 300
                    SplitView.minimumHeight: 180
                    orientation: Qt.Horizontal

                    Rectangle {
                        SplitView.fillWidth: true
                        color: theme.background
                        border.color: theme.borderSubtle
                        border.width: 1

                        ViewerTabs {
                            anchors.fill: parent
                            theme: theme
                            viewerModel: window.viewerModel
                        }
                    }

                    Rectangle {
                        SplitView.preferredWidth: 300
                        SplitView.minimumWidth: 240
                        color: theme.background
                        border.color: theme.borderSubtle
                        border.width: 1

                        TaskPanel {
                            anchors.fill: parent
                            theme: theme
                            controller: window.app
                            taskModel: window.taskModel
                        }
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 30
            color: theme.surface
            border.color: theme.borderSubtle
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: theme.space3
                anchors.rightMargin: theme.space3
                spacing: theme.space3

                Text {
                    Layout.fillWidth: true
                    text: window.app ? window.app.statusText : ""
                    color: theme.textMuted
                    font.family: theme.uiFont
                    font.pixelSize: 11
                    elide: Text.ElideRight
                    Accessible.name: window.app ? "Status: " + window.app.statusText : "Status"
                }

                Text {
                    text: "Source: read-only"
                    color: theme.success
                    font.family: theme.uiFont
                    font.pixelSize: 11
                    font.weight: Font.Medium
                }
            }
        }
    }
}
