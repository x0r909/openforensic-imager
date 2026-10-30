#include "main_window.h"

#include <QAction>
#include <QFileDialog>
#include <QMenuBar>
#include <QQuickWidget>
#include <QQmlContext>
#include <QQmlEngine>
#include <QStatusBar>
#include <QToolBar>
#include <QUrl>

#include "app_controller.h"

MainWindow::MainWindow(AppController *controller, QWidget *parent)
    : QMainWindow(parent), m_controller(controller) {
    setWindowTitle(QStringLiteral("OpenForensic Imager"));
    resize(1280, 820);
    setMinimumSize(960, 640);

    buildMenus();

    m_quick = new QQuickWidget(this);
    m_quick->setResizeMode(QQuickWidget::SizeRootObjectToView);
    m_quick->setInitialProperties({
        {QStringLiteral("app"), QVariant::fromValue(m_controller)},
        {QStringLiteral("evidenceModel"), QVariant::fromValue(m_controller->evidenceModel())},
        {QStringLiteral("fileModel"), QVariant::fromValue(m_controller->fileModel())},
        {QStringLiteral("propertiesModel"),
         QVariant::fromValue(m_controller->propertiesModel())},
        {QStringLiteral("taskModel"), QVariant::fromValue(m_controller->taskModel())},
        {QStringLiteral("viewerModel"), QVariant::fromValue(m_controller->viewer())},
    });
    m_quick->setSource(QUrl(QStringLiteral("qrc:/qml/App.qml")));
    setCentralWidget(m_quick);

    statusBar()->showMessage(m_controller->statusText());
    connect(m_controller, &AppController::statusTextChanged, this, [this]() {
        statusBar()->showMessage(m_controller->statusText());
    });
    connect(m_controller, &AppController::errorRaised, this,
            [this](const QString &message) {
                statusBar()->showMessage(message, 8000);
            });
    connect(m_controller, &AppController::darkChanged, this,
            &MainWindow::applyTheme);
    connect(m_controller, &AppController::imageRequested, this, [this]() {
        const QString path = QFileDialog::getOpenFileName(
            this,
            QStringLiteral("Add Disk Image"),
            QString(),
            QStringLiteral("Forensic images (*.raw *.dd *.E01 *.e01);;All files (*)"));
        if (!path.isEmpty()) {
            m_controller->openImage(path);
        }
    });
    connect(m_controller, &AppController::acquisitionSourceRequested, this, [this]() {
        const QString path = QFileDialog::getOpenFileName(
            this, QStringLiteral("Acquisition Source"), QString(),
            QStringLiteral("Disk images and devices (*.raw *.dd *.E01 *.e01);;All files (*)"));
        if (!path.isEmpty()) {
            m_controller->startAcquisition(path);
        }
    });
    connect(m_controller, &AppController::verificationImageRequested, this, [this]() {
        const QString path = QFileDialog::getOpenFileName(
            this, QStringLiteral("Verify Image"), QString(),
            QStringLiteral("Forensic images (*.raw *.dd *.E01 *.e01);;All files (*)"));
        if (!path.isEmpty()) {
            m_controller->setVerificationPath(path);
            m_controller->startVerification(path);
        }
    });
    connect(m_controller, &AppController::acquisitionDestinationRequested, this, [this]() {
        const QString path =
            QFileDialog::getExistingDirectory(this, QStringLiteral("Acquisition Destination"));
        if (!path.isEmpty()) {
            m_controller->setAcquisitionDestination(path);
        }
    });
    connect(m_controller, &AppController::reportsDirectoryRequested, this, [this]() {
        const QString path =
            QFileDialog::getExistingDirectory(this, QStringLiteral("Reports Directory"));
        if (!path.isEmpty()) {
            m_controller->setReportsDirectory(path);
        }
    });
    connect(m_controller, &AppController::caseDatabaseRequested, this, [this]() {
        const QString path = QFileDialog::getSaveFileName(
            this, QStringLiteral("Case Database"), QString(),
            QStringLiteral("SQLite database (*.db)"));
        if (!path.isEmpty()) {
            m_controller->setCaseDatabasePath(path);
        }
    });
    applyTheme();
}

bool MainWindow::loadQml() {
    return m_quick != nullptr && m_quick->status() == QQuickWidget::Ready;
}

void MainWindow::buildMenus() {
    QMenu *fileMenu = menuBar()->addMenu(QStringLiteral("&File"));
    QAction *addImage = fileMenu->addAction(QStringLiteral("Add Disk &Image…"));
    addImage->setShortcut(QKeySequence(QStringLiteral("Ctrl+O")));
    connect(addImage, &QAction::triggered, m_controller, &AppController::addImage);

    QAction *clearTasks = fileMenu->addAction(QStringLiteral("Clear Completed &Tasks"));
    connect(clearTasks, &QAction::triggered, m_controller,
            &AppController::clearCompletedTasks);

    QMenu *acquisitionMenu = menuBar()->addMenu(QStringLiteral("&Acquisition"));
    QAction *acquire =
        acquisitionMenu->addAction(QStringLiteral("Create Disk &Image…"));
    acquire->setShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+A")));
    connect(acquire, &QAction::triggered, m_controller,
            &AppController::requestAcquisitionSource);
    QAction *verify = acquisitionMenu->addAction(QStringLiteral("&Verify Image…"));
    verify->setShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+V")));
    connect(verify, &QAction::triggered, m_controller,
            &AppController::requestVerificationImage);

    QMenu *caseMenu = menuBar()->addMenu(QStringLiteral("&Case"));
    QAction *casePanel =
        caseMenu->addAction(QStringLiteral("Case and &Acquisition…"));
    connect(casePanel, &QAction::triggered, m_controller,
            &AppController::requestCasePanel);
    caseMenu->addSeparator();
    QAction *createCase = caseMenu->addAction(QStringLiteral("&Create Case"));
    connect(createCase, &QAction::triggered, m_controller, &AppController::createCase);
    QAction *addEvidence =
        caseMenu->addAction(QStringLiteral("Add Selected Evidence to &Case"));
    connect(addEvidence, &QAction::triggered, m_controller,
            &AppController::addEvidenceToCase);
    QAction *chooseDatabase =
        caseMenu->addAction(QStringLiteral("Choose Case &Database…"));
    connect(chooseDatabase, &QAction::triggered, m_controller,
            &AppController::requestCaseDatabase);
    QAction *chooseReports =
        caseMenu->addAction(QStringLiteral("Choose &Reports Directory…"));
    connect(chooseReports, &QAction::triggered, m_controller,
            &AppController::requestReportsDirectory);

    fileMenu->addSeparator();
    QAction *quit = fileMenu->addAction(QStringLiteral("&Quit"));
    quit->setShortcut(QKeySequence::Quit);
    connect(quit, &QAction::triggered, this, &QWidget::close);

    QMenu *viewMenu = menuBar()->addMenu(QStringLiteral("&View"));
    QAction *toggleTheme = viewMenu->addAction(QStringLiteral("Toggle &Theme"));
    toggleTheme->setShortcut(QKeySequence(QStringLiteral("Ctrl+Shift+T")));
    connect(toggleTheme, &QAction::triggered, this, [this]() {
        m_controller->setDark(!m_controller->dark());
    });

    QMenu *helpMenu = menuBar()->addMenu(QStringLiteral("&Help"));
    QAction *about = helpMenu->addAction(QStringLiteral("&About OpenForensic Imager"));
    connect(about, &QAction::triggered, this, [this]() {
        statusBar()->showMessage(
            QStringLiteral("OpenForensic Imager 0.1.0 — evidence is opened read-only"),
            8000);
    });

    QToolBar *toolbar = addToolBar(QStringLiteral("Main"));
    toolbar->setMovable(false);
    toolbar->setToolButtonStyle(Qt::ToolButtonTextOnly);
    toolbar->addAction(addImage);
    toolbar->addSeparator();
    toolbar->addAction(acquire);
    toolbar->addAction(verify);
    toolbar->addSeparator();
    toolbar->addAction(toggleTheme);
}

void MainWindow::applyTheme() {
    const bool dark = m_controller->dark();
    const QColor background(dark ? QStringLiteral("#141311") : QStringLiteral("#F4F1EA"));
    const QColor text(dark ? QStringLiteral("#F3EEE4") : QStringLiteral("#1C1B18"));
    const QColor accent(dark ? QStringLiteral("#E0A458") : QStringLiteral("#8A4B00"));
    QPalette palette;
    palette.setColor(QPalette::Window, background);
    palette.setColor(QPalette::WindowText, text);
    palette.setColor(QPalette::Base, dark ? QColor(QStringLiteral("#1D1B18"))
                                          : QColor(QStringLiteral("#FBFAF6")));
    palette.setColor(QPalette::AlternateBase, background);
    palette.setColor(QPalette::Text, text);
    palette.setColor(QPalette::Button, background);
    palette.setColor(QPalette::ButtonText, text);
    palette.setColor(QPalette::Highlight, accent);
    palette.setColor(QPalette::HighlightedText,
                     dark ? QColor(QStringLiteral("#1A1712"))
                          : QColor(QStringLiteral("#FFFFFF")));
    palette.setColor(QPalette::ToolTipBase, background);
    palette.setColor(QPalette::ToolTipText, text);
    setPalette(palette);
    menuBar()->setPalette(palette);
    statusBar()->setPalette(palette);
}
