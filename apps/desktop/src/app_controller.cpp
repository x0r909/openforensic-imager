#include "app_controller.h"

#include <QDateTime>
#include <QFileDialog>
#include <QFileInfo>
#include <QJsonArray>
#include <QUuid>

AppController::AppController(QObject *parent) : QObject(parent) {
    m_pollTimer.setInterval(250);
    connect(&m_pollTimer, &QTimer::timeout, this, &AppController::pollJobs);
    if (!m_abi.isValid()) {
        m_statusText = QStringLiteral("Native service unavailable: ") + m_abi.errorString();
    }
}

void AppController::setDark(bool dark) {
    if (m_dark == dark) {
        return;
    }
    m_dark = dark;
    emit darkChanged();
}

bool AppController::canNavigateUp() const {
    return m_currentDirectoryInode != 0 && !m_directoryStack.isEmpty();
}

void AppController::setStatus(const QString &text) {
    if (m_statusText == text) {
        return;
    }
    m_statusText = text;
    emit statusTextChanged();
}

void AppController::addImage() {
    emit imageRequested();
}

void AppController::openImage(const QString &path) {
    if (path.isEmpty()) {
        return;
    }
    identifyImage(path);
}

void AppController::identifyImage(const QString &path) {
    QString error;
    const QString jobId = m_abi.submit(
        {{QStringLiteral("operation"), QStringLiteral("image_identify")},
         {QStringLiteral("path"), path}},
        &error);
    if (jobId.isEmpty()) {
        emit errorRaised(error);
        setStatus(QStringLiteral("Unable to open image"));
        return;
    }
    trackJob(jobId,
             {QStringLiteral("Identify image"), QStringLiteral("image_identify"),
              QUuid::createUuid().toString(QUuid::WithoutBraces), path, 0, 0, {}});
    setStatus(QStringLiteral("Opening image"));
}

void AppController::analyzePartitions(const QString &imageId, const QString &path) {
    QString error;
    const QString jobId = m_abi.submit(
        {{QStringLiteral("operation"), QStringLiteral("partition_analysis")},
         {QStringLiteral("path"), path}},
        &error);
    if (jobId.isEmpty()) {
        emit errorRaised(error);
        return;
    }
    trackJob(jobId,
             {QStringLiteral("Analyze partitions"), QStringLiteral("partition_analysis"),
              imageId, path, 0, 0, {}});
}

void AppController::openFilesystem(const QString &imageId, const QString &path,
                                   quint64 offset) {
    QString error;
    const QString jobId = m_abi.submit(
        {{QStringLiteral("operation"), QStringLiteral("filesystem_open")},
         {QStringLiteral("path"), path},
         {QStringLiteral("offset_bytes"), static_cast<qint64>(offset)}},
        &error);
    if (jobId.isEmpty()) {
        emit errorRaised(error);
        return;
    }
    trackJob(jobId,
             {QStringLiteral("Open filesystem"), QStringLiteral("filesystem_open"),
              imageId, path, offset, 0, {}});
}

void AppController::listDirectory(quint64 inode, const QString &label) {
    if (m_currentFilesystemPath.isEmpty()) {
        return;
    }
    m_files.setLoading(label);
    QString error;
    const QString jobId = m_abi.submit(
        {{QStringLiteral("operation"), QStringLiteral("filesystem_list")},
         {QStringLiteral("path"), m_currentFilesystemPath},
         {QStringLiteral("offset_bytes"), static_cast<qint64>(m_currentFilesystemOffset)},
         {QStringLiteral("inode"), static_cast<qint64>(inode)}},
        &error);
    if (jobId.isEmpty()) {
        m_files.setError(error);
        return;
    }
    trackJob(jobId,
             {QStringLiteral("List directory"), QStringLiteral("filesystem_list"),
              m_selectedImageId, m_currentFilesystemPath, m_currentFilesystemOffset, inode,
              label});
}

void AppController::activateNode(const QVariantMap &node) {
    if (node.isEmpty()) {
        return;
    }
    const QString id = node.value(QStringLiteral("id")).toString();
    const QString path = node.value(QStringLiteral("path")).toString();
    const quint64 offset = node.value(QStringLiteral("offset")).toULongLong();
    const QVariantMap payload = node.value(QStringLiteral("payload")).toMap();
    m_selectedImageId = id;
    m_selectedImagePath = path;
    if (node.value(QStringLiteral("image")).toBool()) {
        m_properties.setFromImage(path);
        setStatus(QStringLiteral("Image selected"));
        analyzePartitions(id, path);
        return;
    }
    if (node.value(QStringLiteral("partition")).toBool()) {
        m_properties.setFromPartition(QJsonObject::fromVariantMap(payload));
        openFilesystem(id, path, offset);
        setStatus(QStringLiteral("Opening partition"));
        return;
    }
    if (payload.value(QStringLiteral("kind")).toString().length() > 0) {
        m_currentFilesystemPath = path;
        m_currentFilesystemOffset = offset;
        const quint64 root = payload.value(QStringLiteral("root_inode")).toULongLong();
        m_currentDirectoryInode = root;
        m_directoryStack.clear();
        listDirectory(root, QStringLiteral("/"));
        emit navigationChanged();
    }
}

void AppController::selectFile(const QVariantMap &entry) {
    m_properties.setFromEntry(QJsonObject::fromVariantMap(entry));
    if (!entry.value(QStringLiteral("is_directory")).toBool()) {
        m_viewer.setLoading(QStringLiteral("Loading preview"));
        loadPreview(entry, QStringLiteral("hex"));
    }
    setStatus(QStringLiteral("File selected"));
}

void AppController::openFile(const QVariantMap &entry) {
    if (entry.value(QStringLiteral("is_directory")).toBool()) {
        m_directoryStack.append(m_currentDirectoryInode);
        m_currentDirectoryInode =
            entry.value(QStringLiteral("metadata_address")).toULongLong();
        listDirectory(m_currentDirectoryInode,
                      entry.value(QStringLiteral("name")).toString());
        emit navigationChanged();
    }
}

void AppController::navigateUp() {
    if (m_directoryStack.isEmpty()) {
        return;
    }
    m_currentDirectoryInode = m_directoryStack.takeLast();
    listDirectory(m_currentDirectoryInode, QStringLiteral("/"));
    emit navigationChanged();
}

void AppController::cancelJob(const QString &jobId) {
    m_abi.cancel(jobId);
}

void AppController::clearCompletedTasks() {
    m_tasks.removeCompleted();
}

void AppController::trackJob(const QString &jobId, const PendingJob &pending) {
    m_pending.insert(jobId, pending);
    m_tasks.addOrUpdate(jobId, pending.title, {{QStringLiteral("status"), QStringLiteral("pending")}});
    if (!m_pollTimer.isActive()) {
        m_pollTimer.start();
    }
}

void AppController::pollJobs() {
    if (m_pending.isEmpty()) {
        m_pollTimer.stop();
        return;
    }
    const QStringList ids = m_pending.keys();
    for (const QString &jobId : ids) {
        QString error;
        const QJsonObject envelope = m_abi.snapshot(jobId, &error);
        if (envelope.isEmpty()) {
            continue;
        }
        handleJobUpdate(jobId, envelope.value(QStringLiteral("body")).toObject());
    }
}

void AppController::handleJobUpdate(const QString &jobId, const QJsonObject &snapshot) {
    const PendingJob pending = m_pending.value(jobId);
    m_tasks.addOrUpdate(jobId, pending.title, snapshot);
    const QString status = snapshot.value(QStringLiteral("status")).toString();
    const bool terminal = status == QStringLiteral("succeeded")
        || status == QStringLiteral("partial")
        || status == QStringLiteral("failed")
        || status == QStringLiteral("cancelled");
    if (!terminal) {
        return;
    }
    m_pending.remove(jobId);
    if (status == QStringLiteral("failed")) {
        emit errorRaised(snapshot.value(QStringLiteral("message")).toString());
        setStatus(QStringLiteral("Task failed"));
        return;
    }
    if (status == QStringLiteral("cancelled")) {
        setStatus(QStringLiteral("Task cancelled"));
        return;
    }
    const QJsonObject result = snapshot.value(QStringLiteral("result")).toObject();
    if (pending.operation == QStringLiteral("image_identify")) {
        const QString format = result.value(QStringLiteral("format")).toString().toUpper();
        const quint64 length =
            result.value(QStringLiteral("logical_length")).toVariant().toULongLong();
        const QString imageId = pending.imageId;
        const QJsonArray images = {
            QJsonObject{{QStringLiteral("id"), imageId},
                        {QStringLiteral("name"), QFileInfo(pending.imagePath).fileName()},
                        {QStringLiteral("format"), format},
                        {QStringLiteral("path"), pending.imagePath},
                        {QStringLiteral("length"), static_cast<qint64>(length)}}};
        m_evidence.setImages(images);
        m_selectedImageId = imageId;
        m_selectedImagePath = pending.imagePath;
        m_properties.setFromImage(pending.imagePath);
        analyzePartitions(imageId, pending.imagePath);
        setStatus(QStringLiteral("Image opened read-only"));
    } else if (pending.operation == QStringLiteral("partition_analysis")) {
        m_evidence.setPartitions(pending.imageId,
                                 result.value(QStringLiteral("partitions")).toArray());
    } else if (pending.operation == QStringLiteral("filesystem_open")) {
        QJsonObject filesystem = result;
        filesystem.insert(QStringLiteral("path"), pending.imagePath);
        filesystem.insert(QStringLiteral("offset_bytes"),
                          static_cast<qint64>(pending.offset));
        m_evidence.setFilesystem(pending.imageId, filesystem);
        m_currentFilesystemPath = pending.imagePath;
        m_currentFilesystemOffset = pending.offset;
        m_currentDirectoryInode =
            result.value(QStringLiteral("root_inode")).toVariant().toULongLong();
        m_directoryStack.clear();
        listDirectory(m_currentDirectoryInode, QStringLiteral("/"));
        emit navigationChanged();
    } else if (pending.operation == QStringLiteral("filesystem_list")) {
        m_files.setEntries(pending.location, result.value(QStringLiteral("entries")).toArray());
        setStatus(QStringLiteral("Directory loaded"));
    } else if (pending.operation == QStringLiteral("hex_preview")) {
        m_viewer.setHex(result);
    } else if (pending.operation == QStringLiteral("text_preview")) {
        m_viewer.setText(result);
    } else if (pending.operation == QStringLiteral("file_signature")) {
        m_viewer.setSignature(result);
    }
}

void AppController::loadPreview(const QVariantMap &entry, const QString &tab) {
    Q_UNUSED(tab)
    const quint64 metadataAddress =
        entry.value(QStringLiteral("metadata_address")).toULongLong();
    if (m_currentFilesystemPath.isEmpty() || metadataAddress == 0) {
        return;
    }
    QString error;
    const QString hexJob = m_abi.submit(
        {{QStringLiteral("operation"), QStringLiteral("hex_preview")},
         {QStringLiteral("path"), m_currentFilesystemPath},
         {QStringLiteral("offset_bytes"),
          static_cast<qint64>(m_currentFilesystemOffset)},
         {QStringLiteral("metadata_address"), static_cast<qint64>(metadataAddress)},
         {QStringLiteral("file_offset"), 0},
         {QStringLiteral("length"), 4096}},
        &error);
    if (!hexJob.isEmpty()) {
        trackJob(hexJob, {QStringLiteral("Hex preview"), QStringLiteral("hex_preview"),
                          m_selectedImageId, m_currentFilesystemPath,
                          m_currentFilesystemOffset, metadataAddress, {}});
    }
    const QString textJob = m_abi.submit(
        {{QStringLiteral("operation"), QStringLiteral("text_preview")},
         {QStringLiteral("path"), m_currentFilesystemPath},
         {QStringLiteral("offset_bytes"),
          static_cast<qint64>(m_currentFilesystemOffset)},
         {QStringLiteral("metadata_address"), static_cast<qint64>(metadataAddress)},
         {QStringLiteral("file_offset"), 0},
         {QStringLiteral("max_bytes"), 65536}},
        &error);
    if (!textJob.isEmpty()) {
        trackJob(textJob, {QStringLiteral("Text preview"), QStringLiteral("text_preview"),
                           m_selectedImageId, m_currentFilesystemPath,
                           m_currentFilesystemOffset, metadataAddress, {}});
    }
    const QString signatureJob = m_abi.submit(
        {{QStringLiteral("operation"), QStringLiteral("file_signature")},
         {QStringLiteral("path"), m_currentFilesystemPath},
         {QStringLiteral("offset_bytes"),
          static_cast<qint64>(m_currentFilesystemOffset)},
         {QStringLiteral("metadata_address"), static_cast<qint64>(metadataAddress)}},
        &error);
    if (!signatureJob.isEmpty()) {
        trackJob(signatureJob, {QStringLiteral("Identify file signature"),
                                QStringLiteral("file_signature"), m_selectedImageId,
                                m_currentFilesystemPath, m_currentFilesystemOffset,
                                metadataAddress, {}});
    }
    m_viewer.setMetadata(QStringLiteral("Metadata address: %1\nPath: %2")
                             .arg(metadataAddress)
                             .arg(entry.value(QStringLiteral("name")).toString()));
}

QString AppController::currentImageId() const {
    return m_selectedImageId;
}
