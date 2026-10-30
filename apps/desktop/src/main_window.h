#pragma once

#include <QMainWindow>

class AppController;
class QQuickWidget;

class MainWindow : public QMainWindow {
    Q_OBJECT

public:
    explicit MainWindow(AppController *controller, QWidget *parent = nullptr);

    bool loadQml();
    QQuickWidget *quickWidget() const { return m_quick; }

private:
    void buildMenus();
    void applyTheme();

    AppController *m_controller;
    QQuickWidget *m_quick = nullptr;
};
