#include <QApplication>
#include <QCommandLineParser>
#include <QPixmap>
#include <QQuickStyle>
#include <QTimer>

#include "app_controller.h"
#include "main_window.h"

int main(int argc, char *argv[]) {
    QApplication application(argc, argv);
    QApplication::setApplicationName(QStringLiteral("OpenForensic Imager"));
    QApplication::setApplicationVersion(QStringLiteral("0.1.0"));
    QApplication::setOrganizationName(QStringLiteral("OpenForensic"));
    QQuickStyle::setStyle(QStringLiteral("Basic"));

    QCommandLineParser parser;
    parser.setApplicationDescription(
        QStringLiteral("Linux-native forensic acquisition and evidence analysis"));
    parser.addHelpOption();
    parser.addVersionOption();
    QCommandLineOption selfTest(QStringLiteral("self-test"),
                                QStringLiteral("Load the interface and exit."));
    parser.addOption(selfTest);
    QCommandLineOption dark(QStringLiteral("dark"),
                            QStringLiteral("Start with the dark theme."));
    parser.addOption(dark);
    QCommandLineOption screenshot(
        QStringLiteral("screenshot"),
        QStringLiteral("Save a window screenshot and exit."),
        QStringLiteral("path"));
    parser.addOption(screenshot);
    parser.process(application);

    AppController controller;
    controller.setDark(parser.isSet(dark));
    MainWindow window(&controller);
    window.show();
    if (!window.loadQml()) {
        return 2;
    }
    if (parser.isSet(selfTest)) {
        QTimer::singleShot(300, &application, &QCoreApplication::quit);
    }
    if (parser.isSet(screenshot)) {
        const QString path = parser.value(screenshot);
        QTimer::singleShot(500, &window, [&window, path]() {
            window.grab().save(path);
            QCoreApplication::quit();
        });
    }
    return application.exec();
}
