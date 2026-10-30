pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root

    property var theme
    property var viewerModel
    color: root.theme.background

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: root.theme.space2
        spacing: root.theme.space2

        RowLayout {
            Layout.fillWidth: true
            spacing: root.theme.space2

            TabBar {
                id: tabs
                Layout.fillWidth: true
                Accessible.name: "Viewer tabs"
                TabButton { text: "Hex" }
                TabButton { text: "Text" }
                TabButton { text: "Metadata" }
                TabButton { text: "Signature" }
            }

            Text {
                visible: root.viewerModel.loading
                text: "Loading preview"
                color: root.theme.textMuted
                font.family: root.theme.uiFont
                font.pixelSize: 11
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: tabs.currentIndex

            Item {
                ListView {
                    anchors.fill: parent
                    clip: true
                    model: root.viewerModel.hexRows
                    Accessible.name: "Hexadecimal viewer"
                    delegate: Row {
                        id: hexRow
                        required property var modelData
                        width: ListView.view.width
                        height: 20
                        Text { width: 90; text: hexRow.modelData.offset; color: root.theme.textMuted; font.family: root.theme.monoFont; font.pixelSize: 11 }
                        Text { width: 390; text: hexRow.modelData.hex; color: root.theme.text; font.family: root.theme.monoFont; font.pixelSize: 11; elide: Text.ElideRight }
                        Text { text: hexRow.modelData.ascii; color: root.theme.textMuted; font.family: root.theme.monoFont; font.pixelSize: 11; elide: Text.ElideRight }
                    }
                }
                Text {
                    anchors.centerIn: parent
                    visible: root.viewerModel.hexRows.length === 0
                    text: "Select a file to page through its bytes."
                    color: root.theme.textMuted
                    font.family: root.theme.uiFont
                    font.pixelSize: 12
                }
            }

            Item {
                ScrollView {
                    anchors.fill: parent
                    TextArea {
                        readOnly: true
                        selectByMouse: true
                        text: root.viewerModel.text
                        color: root.theme.text
                        font.family: root.theme.monoFont
                        font.pixelSize: 11
                        wrapMode: TextEdit.Wrap
                        background: Rectangle { color: root.theme.surface; border.color: root.theme.borderSubtle; border.width: 1 }
                        Accessible.name: "Text preview"
                    }
                }
            }

            Item {
                ScrollView {
                    anchors.fill: parent
                    TextArea {
                        readOnly: true
                        selectByMouse: true
                        text: root.viewerModel.metadata
                        color: root.theme.text
                        font.family: root.theme.uiFont
                        font.pixelSize: 12
                        wrapMode: TextEdit.Wrap
                        background: Rectangle { color: root.theme.surface; border.color: root.theme.borderSubtle; border.width: 1 }
                        Accessible.name: "File metadata"
                    }
                }
            }

            Item {
                ColumnLayout {
                    anchors.fill: parent
                    spacing: root.theme.space2
                    Text {
                        text: root.viewerModel.signature.length > 0 ? root.viewerModel.signature : "No file selected"
                        color: root.theme.text
                        font.family: root.theme.uiFont
                        font.pixelSize: 13
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                    Text {
                        text: root.viewerModel.status
                        color: root.theme.textMuted
                        font.family: root.theme.uiFont
                        font.pixelSize: 11
                        wrapMode: Text.WordWrap
                        Layout.fillWidth: true
                    }
                    Item { Layout.fillHeight: true }
                }
            }
        }
    }
}
