#pragma once

#include <QJsonObject>
#include <QObject>
#include <QString>
#include <QStringList>
#include <QTimer>

#include "abi_client.h"
#include "models.h"

class AppController : public QObject {
    Q_OBJECT
    Q_PROPERTY(EvidenceTreeModel *evidenceModel READ evidenceModel CONSTANT)
    Q_PROPERTY(FileListModel *fileModel READ fileModel CONSTANT)
    Q_PROPERTY(PropertiesModel *propertiesModel READ propertiesModel CONSTANT)
    Q_PROPERTY(TaskModel *taskModel READ taskModel CONSTANT)
    Q_PROPERTY(ViewerModel *viewer READ viewer CONSTANT)
    Q_PROPERTY(bool dark READ dark WRITE setDark NOTIFY darkChanged)
    Q_PROPERTY(bool canNavigateUp READ canNavigateUp NOTIFY navigationChanged)
    Q_PROPERTY(QString statusText READ statusText NOTIFY statusTextChanged)

public:
    explicit AppController(QObject *parent = nullptr);

    EvidenceTreeModel *evidenceModel() { return &m_evidence; }
    FileListModel *fileModel() { return &m_files; }
    PropertiesModel *propertiesModel() { return &m_properties; }
    TaskModel *taskModel() { return &m_tasks; }
    ViewerModel *viewer() { return &m_viewer; }

    bool dark() const { return m_dark; }
    void setDark(bool dark);
    bool canNavigateUp() const;
    QString statusText() const { return m_statusText; }

    Q_INVOKABLE void addImage();
    Q_INVOKABLE void openImage(const QString &path);
    Q_INVOKABLE void activateNode(const QVariantMap &node);
    Q_INVOKABLE void selectFile(const QVariantMap &entry);
    Q_INVOKABLE void openFile(const QVariantMap &entry);
    Q_INVOKABLE void navigateUp();
    Q_INVOKABLE void cancelJob(const QString &jobId);
    Q_INVOKABLE void clearCompletedTasks();
    Q_INVOKABLE void setStatus(const QString &text);

signals:
    void darkChanged();
    void navigationChanged();
    void statusTextChanged();
    void imageRequested();
    void errorRaised(const QString &message);

private:
    struct PendingJob {
        QString title;
        QString operation;
        QString imageId;
        QString imagePath;
        quint64 offset = 0;
        quint64 inode = 0;
        QString location;
    };

    void trackJob(const QString &jobId, const PendingJob &pending);
    void pollJobs();
    void handleJobUpdate(const QString &jobId, const QJsonObject &snapshot);
    void identifyImage(const QString &path);
    void analyzePartitions(const QString &imageId, const QString &path);
    void openFilesystem(const QString &imageId, const QString &path, quint64 offset);
    void listDirectory(quint64 inode, const QString &label);
    void loadPreview(const QVariantMap &entry, const QString &tab);
    QString currentImageId() const;

    AbiClient m_abi;
    EvidenceTreeModel m_evidence;
    FileListModel m_files;
    PropertiesModel m_properties;
    TaskModel m_tasks;
    ViewerModel m_viewer;
    QTimer m_pollTimer;
    QHash<QString, PendingJob> m_pending;
    QString m_selectedImageId;
    QString m_selectedImagePath;
    QString m_currentFilesystemPath;
    quint64 m_currentFilesystemOffset = 0;
    quint64 m_currentDirectoryInode = 0;
    QList<quint64> m_directoryStack;
    QString m_statusText = QStringLiteral("No evidence loaded");
    bool m_dark = false;
};
