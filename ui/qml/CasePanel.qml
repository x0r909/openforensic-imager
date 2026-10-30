pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root

    property var theme
    property var controller

    color: root.theme.background

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: root.theme.space2
        spacing: root.theme.space2

        Text {
            text: "Case and acquisition"
            color: root.theme.text
            font.family: root.theme.uiFont
            font.pixelSize: 14
            font.weight: Font.DemiBold
        }

        GridLayout {
            Layout.fillWidth: true
            columns: 2
            columnSpacing: root.theme.space2
            rowSpacing: root.theme.space2

            Text { text: "Case UUID"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11 }
            TextField {
                Layout.fillWidth: true
                text: root.controller.caseId
                placeholderText: "00000000-0000-0000-0000-000000000000"
                onEditingFinished: root.controller.caseId = text
                Accessible.name: "Case UUID"
                color: root.theme.text
                font.family: root.theme.monoFont
                font.pixelSize: 11
            }

            Text { text: "Database"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11 }
            TextField {
                Layout.fillWidth: true
                text: root.controller.caseDatabasePath
                placeholderText: "Choose a case database"
                readOnly: true
                Accessible.name: "Case database path"
                color: root.theme.text
                font.family: root.theme.monoFont
                font.pixelSize: 11
            }

            Text { text: "Description"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11 }
            TextField {
                Layout.fillWidth: true
                text: root.controller.evidenceDescription
                placeholderText: "Evidence description"
                onEditingFinished: root.controller.evidenceDescription = text
                Accessible.name: "Evidence description"
                color: root.theme.text
                font.family: root.theme.uiFont
                font.pixelSize: 11
            }

            Text { text: "Destination"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11 }
            TextField {
                Layout.fillWidth: true
                text: root.controller.acquisitionDestination
                placeholderText: "Acquisition destination"
                readOnly: true
                Accessible.name: "Acquisition destination"
                color: root.theme.text
                font.family: root.theme.monoFont
                font.pixelSize: 11
            }

            Text { text: "Base name"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11 }
            TextField {
                Layout.fillWidth: true
                text: root.controller.acquisitionBaseName
                onEditingFinished: root.controller.acquisitionBaseName = text
                Accessible.name: "Acquisition base name"
                color: root.theme.text
                font.family: root.theme.monoFont
                font.pixelSize: 11
            }

            Text { text: "Format"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11 }
            ComboBox {
                Layout.fillWidth: true
                model: ["raw", "ewf1_e01"]
                currentIndex: root.controller.acquisitionFormat === "ewf1_e01" ? 1 : 0
                onActivated: root.controller.acquisitionFormat = currentText
                Accessible.name: "Acquisition format"
            }

            Text { text: "Reports"; color: root.theme.textMuted; font.family: root.theme.uiFont; font.pixelSize: 11 }
            TextField {
                Layout.fillWidth: true
                text: root.controller.reportsDirectory
                placeholderText: "Reports directory"
                readOnly: true
                Accessible.name: "Reports directory"
                color: root.theme.text
                font.family: root.theme.monoFont
                font.pixelSize: 11
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: root.theme.space2

            Button {
                text: "Choose database"
                onClicked: root.controller.requestCaseDatabase()
                Accessible.name: "Choose case database"
                Layout.preferredHeight: root.theme.controlHeight
            }
            Button {
                text: "Choose destination"
                onClicked: root.controller.requestAcquisitionDestination()
                Accessible.name: "Choose acquisition destination"
                Layout.preferredHeight: root.theme.controlHeight
            }
            Button {
                text: "Choose reports"
                onClicked: root.controller.requestReportsDirectory()
                Accessible.name: "Choose reports directory"
                Layout.preferredHeight: root.theme.controlHeight
            }
            Item { Layout.fillWidth: true }
            Button {
                text: "Create case"
                onClicked: root.controller.createCase()
                Accessible.name: "Create case"
                Layout.preferredHeight: root.theme.controlHeight
            }
            Button {
                text: "Add evidence"
                onClicked: root.controller.addEvidenceToCase()
                Accessible.name: "Add selected evidence to case"
                Layout.preferredHeight: root.theme.controlHeight
            }
            Button {
                text: "Acquire"
                onClicked: root.controller.requestAcquisitionSource()
                Accessible.name: "Start acquisition"
                Layout.preferredHeight: root.theme.controlHeight
            }
            Button {
                text: "Verify"
                onClicked: root.controller.requestVerificationImage()
                Accessible.name: "Verify image"
                Layout.preferredHeight: root.theme.controlHeight
            }
        }

        Text {
            Layout.fillWidth: true
            text: "Acquisition writes to the chosen destination only. The source remains read-only, and a successful acquisition is verified before it is reported as complete."
            color: root.theme.textMuted
            font.family: root.theme.uiFont
            font.pixelSize: 10
            wrapMode: Text.WordWrap
        }

        Item { Layout.fillHeight: true }
    }
}
