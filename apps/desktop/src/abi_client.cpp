#include "abi_client.h"

#include <QByteArray>
#include <QJsonDocument>
#include <QJsonValue>

#include "ofi.h"

namespace {

QJsonObject takeResponse(uint8_t *data, size_t length) {
    if (data == nullptr) {
        return {};
    }
    const QByteArray bytes(reinterpret_cast<const char *>(data), static_cast<int>(length));
    ofi_bytes_free(data, length);
    return QJsonDocument::fromJson(bytes).object();
}

QString takeError(ofi_error *error) {
    if (error == nullptr) {
        return {};
    }
    const QString code = QString::fromUtf8(ofi_error_code(error));
    const QString message = QString::fromUtf8(ofi_error_message(error));
    ofi_error_free(error);
    return code.isEmpty() ? message : code + QStringLiteral(": ") + message;
}

} // namespace

AbiClient::AbiClient() {
    ofi_error *error = nullptr;
    if (ofi_job_manager_new(2, &m_manager, &error) != OFI_STATUS_OK) {
        m_error = takeError(error);
        m_manager = nullptr;
    }
}

AbiClient::~AbiClient() {
    if (m_manager != nullptr) {
        ofi_job_manager_free(m_manager);
    }
}

bool AbiClient::isValid() const {
    return m_manager != nullptr;
}

QString AbiClient::errorString() const {
    return m_error;
}

QString AbiClient::submit(const QJsonObject &request, QString *error) {
    if (m_manager == nullptr) {
        if (error != nullptr) {
            *error = m_error;
        }
        return {};
    }
    const QByteArray bytes = QJsonDocument(request).toJson(QJsonDocument::Compact);
    uint8_t *response = nullptr;
    size_t responseLength = 0;
    ofi_error *ffiError = nullptr;
    const ofi_status status = ofi_job_submit(
        m_manager,
        reinterpret_cast<const uint8_t *>(bytes.constData()),
        static_cast<size_t>(bytes.size()),
        &response,
        &responseLength,
        &ffiError);
    if (status != OFI_STATUS_OK) {
        if (error != nullptr) {
            *error = takeError(ffiError);
        }
        return {};
    }
    const QJsonObject body = takeResponse(response, responseLength);
    return body.value(QStringLiteral("body")).toObject()
        .value(QStringLiteral("job_id")).toString();
}

QJsonObject AbiClient::snapshot(const QString &jobId, QString *error) {
    if (m_manager == nullptr) {
        if (error != nullptr) {
            *error = m_error;
        }
        return {};
    }
    const QByteArray id = jobId.toUtf8();
    uint8_t *response = nullptr;
    size_t responseLength = 0;
    ofi_error *ffiError = nullptr;
    const ofi_status status = ofi_job_snapshot(
        m_manager,
        id.constData(),
        &response,
        &responseLength,
        &ffiError);
    if (status != OFI_STATUS_OK) {
        if (error != nullptr) {
            *error = takeError(ffiError);
        }
        return {};
    }
    return takeResponse(response, responseLength);
}

bool AbiClient::cancel(const QString &jobId) {
    if (m_manager == nullptr) {
        return false;
    }
    const QByteArray id = jobId.toUtf8();
    uint8_t *response = nullptr;
    size_t responseLength = 0;
    ofi_error *error = nullptr;
    const ofi_status status = ofi_job_cancel(
        m_manager, id.constData(), &response, &responseLength, &error);
    if (status != OFI_STATUS_OK) {
        takeError(error);
        return false;
    }
    const QJsonObject body = takeResponse(response, responseLength);
    return body.value(QStringLiteral("body"))
        .toObject()
        .value(QStringLiteral("cancelled"))
        .toBool();
}

QJsonObject AbiClient::createCase(const QJsonObject &request, QString *error) {
    const QByteArray bytes = QJsonDocument(request).toJson(QJsonDocument::Compact);
    uint8_t *response = nullptr;
    size_t responseLength = 0;
    ofi_error *ffiError = nullptr;
    const ofi_status status = ofi_case_create(
        reinterpret_cast<const uint8_t *>(bytes.constData()),
        static_cast<size_t>(bytes.size()),
        &response,
        &responseLength,
        &ffiError);
    if (status != OFI_STATUS_OK) {
        if (error != nullptr) {
            *error = takeError(ffiError);
        }
        return {};
    }
    return takeResponse(response, responseLength);
}

QJsonObject AbiClient::addEvidence(const QJsonObject &request, QString *error) {
    const QByteArray bytes = QJsonDocument(request).toJson(QJsonDocument::Compact);
    uint8_t *response = nullptr;
    size_t responseLength = 0;
    ofi_error *ffiError = nullptr;
    const ofi_status status = ofi_evidence_add(
        reinterpret_cast<const uint8_t *>(bytes.constData()),
        static_cast<size_t>(bytes.size()),
        &response,
        &responseLength,
        &ffiError);
    if (status != OFI_STATUS_OK) {
        if (error != nullptr) {
            *error = takeError(ffiError);
        }
        return {};
    }
    return takeResponse(response, responseLength);
}
