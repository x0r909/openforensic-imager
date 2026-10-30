pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts

Rectangle {
    id: root

    property var theme
    property var propertiesModel
    color: root.theme.background

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: root.theme.space2
        spacing: root.theme.space2

        Text {
            text: "Properties"
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
            model: root.propertiesModel
            focus: true
            activeFocusOnTab: true
            Accessible.name: "Evidence properties"
            Accessible.description: "Metadata, hashes, and timestamp provenance"

            delegate: Column {
                id: propertyRow
                required property string group
                required property bool groupStart
                required property string key
                required property string value
                required property string provenance
                required property bool monospace
                width: ListView.view.width
                spacing: root.theme.space1

                Text {
                    visible: propertyRow.groupStart
                    width: parent.width
                    text: propertyRow.group
                    color: root.theme.text
                    font.family: root.theme.uiFont
                    font.pixelSize: 11
                    font.weight: Font.DemiBold
                    topPadding: root.theme.space2
                }

                RowLayout {
                    width: parent.width
                    spacing: root.theme.space2
                    Text {
                        text: propertyRow.key
                        color: root.theme.textMuted
                        font.family: root.theme.uiFont
                        font.pixelSize: 11
                        Layout.preferredWidth: 92
                        elide: Text.ElideRight
                    }
                    Text {
                        text: propertyRow.value
                        color: root.theme.text
                        font.family: propertyRow.monospace ? root.theme.monoFont : root.theme.uiFont
                        font.pixelSize: 11
                        wrapMode: Text.WrapAnywhere
                        Layout.fillWidth: true
                    }
                }

                Text {
                    visible: propertyRow.provenance.length > 0
                    width: parent.width
                    text: propertyRow.provenance
                    color: root.theme.textMuted
                    font.family: root.theme.uiFont
                    font.pixelSize: 10
                    wrapMode: Text.Wrap
                }
            }
        }

        Text {
            visible: root.propertiesModel.rowCount() === 0
            Layout.fillWidth: true
            text: "Select evidence or a file to inspect its metadata, hashes, and timestamp provenance."
            color: root.theme.textMuted
            font.family: root.theme.uiFont
            font.pixelSize: 11
            wrapMode: Text.WordWrap
        }
    }
}
