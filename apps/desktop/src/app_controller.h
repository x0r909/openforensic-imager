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
    Q_PROPERTY(QString caseDatabasePath READ caseDatabasePath WRITE setCaseDatabasePath
               NOTIFY caseDatabasePathChanged)
    Q_PROPERTY(QString caseId READ caseId WRITE setCaseId NOTIFY caseIdChanged)
    Q_PROPERTY(QString evidenceDescription READ evidenceDescription WRITE
               setEvidenceDescription NOTIFY evidenceDescriptionChanged)
    Q_PROPERTY(QString acquisitionDestination READ acquisitionDestination WRITE
               setAcquisitionDestination NOTIFY acquisitionDestinationChanged)
    Q_PROPERTY(QString acquisitionBaseName READ acquisitionBaseName WRITE
               setAcquisitionBaseName NOTIFY acquisitionBaseNameChanged)
    Q_PROPERTY(QString acquisitionFormat READ acquisitionFormat WRITE
               setAcquisitionFormat NOTIFY acquisitionFormatChanged)
    Q_PROPERTY(QString verificationExpectedSha256 READ verificationExpectedSha256 WRITE
               setVerificationExpectedSha256 NOTIFY verificationExpectedSha256Changed)
    Q_PROPERTY(QString verificationPath READ verificationPath WRITE setVerificationPath
               NOTIFY verificationPathChanged)
    Q_PROPERTY(QString reportsDirectory READ reportsDirectory WRITE setReportsDirectory
               NOTIFY reportsDirectoryChanged)

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
    QString caseDatabasePath() const { return m_caseDatabasePath; }
    void setCaseDatabasePath(const QString &path);
    QString caseId() const { return m_caseId; }
    void setCaseId(const QString &id);
    QString evidenceDescription() const { return m_evidenceDescription; }
    void setEvidenceDescription(const QString &description);
    QString acquisitionDestination() const { return m_acquisitionDestination; }
    void setAcquisitionDestination(const QString &path);
    QString acquisitionBaseName() const { return m_acquisitionBaseName; }
    void setAcquisitionBaseName(const QString &name);
    QString acquisitionFormat() const { return m_acquisitionFormat; }
    void setAcquisitionFormat(const QString &format);
    QString verificationExpectedSha256() const { return m_verificationExpectedSha256; }
    void setVerificationExpectedSha256(const QString &hash);
    QString verificationPath() const { return m_verificationPath; }
    void setVerificationPath(const QString &path);
    QString reportsDirectory() const { return m_reportsDirectory; }
    void setReportsDirectory(const QString &path);

    Q_INVOKABLE void addImage();
    Q_INVOKABLE void openImage(const QString &path);
    Q_INVOKABLE void activateNode(const QVariantMap &node);
    Q_INVOKABLE void selectFile(const QVariantMap &entry);
    Q_INVOKABLE void openFile(const QVariantMap &entry);
    Q_INVOKABLE void navigateUp();
    Q_INVOKABLE void cancelJob(const QString &jobId);
    Q_INVOKABLE void clearCompletedTasks();
    Q_INVOKABLE void setStatus(const QString &text);
    Q_INVOKABLE void createCase();
    Q_INVOKABLE void addEvidenceToCase();
    Q_INVOKABLE void startAcquisition(const QString &sourcePath);
    Q_INVOKABLE void startVerification(const QString &imagePath);
    Q_INVOKABLE void requestAcquisitionSource();
    Q_INVOKABLE void requestVerificationImage();
    Q_INVOKABLE void requestAcquisitionDestination();
    Q_INVOKABLE void requestReportsDirectory();
    Q_INVOKABLE void requestCaseDatabase();
    Q_INVOKABLE void requestCasePanel();

signals:
    void darkChanged();
    void navigationChanged();
    void statusTextChanged();
    void imageRequested();
    void acquisitionSourceRequested();
    void verificationImageRequested();
    void acquisitionDestinationRequested();
    void reportsDirectoryRequested();
    void caseDatabaseRequested();
    void casePanelRequested();
    void errorRaised(const QString &message);
    void caseDatabasePathChanged();
    void caseIdChanged();
    void evidenceDescriptionChanged();
    void acquisitionDestinationChanged();
    void acquisitionBaseNameChanged();
    void acquisitionFormatChanged();
    void verificationExpectedSha256Changed();
    void verificationPathChanged();
    void reportsDirectoryChanged();

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
    QString m_caseDatabasePath;
    QString m_caseId;
    QString m_evidenceDescription;
    QString m_acquisitionDestination;
    QString m_acquisitionBaseName = QStringLiteral("evidence");
    QString m_acquisitionFormat = QStringLiteral("raw");
    QString m_verificationExpectedSha256;
    QString m_verificationPath;
    QString m_reportsDirectory;
    bool m_dark = false;
};
