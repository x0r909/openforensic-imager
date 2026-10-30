pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root

    property var theme
    property var controller
    property var fileModel
    color: root.theme.background

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: root.theme.space2
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: root.theme.controlHeight
            spacing: root.theme.space2

            Button {
                id: upButton
                text: "Up"
                enabled: root.controller.canNavigateUp
                onClicked: root.controller.navigateUp()
                Accessible.name: "Go to parent directory"
                Layout.preferredHeight: root.theme.controlHeight
                padding: root.theme.space2
                background: Rectangle {
                    radius: root.theme.radius
                    color: upButton.hovered ? root.theme.raised : "transparent"
                    border.color: root.theme.border
                    border.width: 1
                }
                contentItem: Text {
                    text: upButton.text
                    color: root.theme.text
                    font.family: root.theme.uiFont
                    font.pixelSize: 12
                }
            }

            Text {
                Layout.fillWidth: true
                text: root.fileModel.locationLabel
                color: root.theme.textMuted
                font.family: root.theme.uiFont
                font.pixelSize: 11
                elide: Text.ElideMiddle
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.borderSubtle
        }

        Row {
            id: header
            Layout.fillWidth: true
            Layout.preferredHeight: 26
            visible: root.fileModel.rowCount() > 0
            Text { width: header.width * 0.42; text: "Name"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11; font.weight: Font.Medium; leftPadding: root.theme.space2; elide: Text.ElideRight }
            Text { width: header.width * 0.12; text: "Size"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11; font.weight: Font.Medium; elide: Text.ElideRight }
            Text { width: header.width * 0.16; text: "Type"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11; font.weight: Font.Medium; elide: Text.ElideRight }
            Text { width: header.width * 0.16; text: "Metadata"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11; font.weight: Font.Medium; elide: Text.ElideRight }
            Text { width: header.width * 0.14; text: "Status"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11; font.weight: Font.Medium; elide: Text.ElideRight }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: root.theme.borderSubtle
            visible: root.fileModel.rowCount() > 0
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: root.fileModel
            focus: true
            activeFocusOnTab: true
            Accessible.name: "File list"
            Accessible.description: "Files in the selected filesystem directory"

            delegate: ItemDelegate {
                id: row
                required property int index
                required property string name
                required property string sizeText
                required property string typeText
                required property string metadataText
                required property string statusText
                required property bool deleted
                required property bool isDirectory
                width: ListView.view.width
                height: root.theme.rowHeight
                padding: 0
                Accessible.name: name
                Accessible.description: typeText + ", " + statusText

                background: Rectangle {
                    color: ListView.isCurrentItem ? root.theme.raised : "transparent"
                    border.width: ListView.isCurrentItem ? 1 : 0
                    border.color: root.theme.accent
                }

                contentItem: Row {
                    width: parent.width
                    Text { width: parent.width * 0.42; text: row.name; color: root.theme.text; font.family: root.theme.uiFont; font.pixelSize: 12; leftPadding: root.theme.space2; elide: Text.ElideRight; anchors.verticalCenter: parent.verticalCenter }
                    Text { width: parent.width * 0.12; text: row.sizeText; color: root.theme.textMuted; font.family: root.theme.monoFont; font.pixelSize: 11; elide: Text.ElideRight; anchors.verticalCenter: parent.verticalCenter }
                    Text { width: parent.width * 0.16; text: row.typeText; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11; elide: Text.ElideRight; anchors.verticalCenter: parent.verticalCenter }
                    Text { width: parent.width * 0.16; text: row.metadataText; color: root.theme.textMuted; font.family: root.theme.monoFont; font.pixelSize: 11; elide: Text.ElideRight; anchors.verticalCenter: parent.verticalCenter }
                    Text { width: parent.width * 0.14; text: row.statusText; color: row.deleted ? root.theme.error : root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11; elide: Text.ElideRight; anchors.verticalCenter: parent.verticalCenter }
                }

                onClicked: {
                    list.currentIndex = index;
                    root.controller.selectFile(root.fileModel.entryAt(index));
                }

                onDoubleClicked: root.controller.openFile(root.fileModel.entryAt(index))
            }
        }

        EmptyState {
            visible: root.fileModel.rowCount() === 0
            Layout.fillWidth: true
            Layout.fillHeight: true
            theme: root.theme
            title: root.fileModel.loading ? "Loading directory" : "No directory selected"
            body: root.fileModel.emptyMessage
            actionText: "Add disk image"
            onActionTriggered: root.controller.addImage()
        }
    }
}
