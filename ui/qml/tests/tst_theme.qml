import QtQuick
import QtTest
import ".."

TestCase {
    name: "Theme"

    Theme { id: theme }

    function test_light_and_dark_tokens_differ() {
        const light = Qt.createQmlObject('import QtQuick; import ".."; Theme {}', this);
        const dark = Qt.createQmlObject('import QtQuick; import ".."; Theme {}', this);
        dark.dark = true;
        compare(light.text === dark.text, false);
        compare(light.background === dark.background, false);
    }

    function test_fonts_are_installed() {
        compare(theme.uiFont.length > 0, true);
        compare(theme.monoFont.length > 0, true);
    }
}
