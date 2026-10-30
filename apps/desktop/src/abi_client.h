#pragma once

#include <QJsonObject>
#include <QString>

struct ofi_job_manager;

class AbiClient {
public:
    AbiClient();
    ~AbiClient();

    AbiClient(const AbiClient &) = delete;
    AbiClient &operator=(const AbiClient &) = delete;

    bool isValid() const;
    QString errorString() const;

    QString submit(const QJsonObject &request, QString *error);
    QJsonObject snapshot(const QString &jobId, QString *error);
    bool cancel(const QString &jobId);
    QJsonObject createCase(const QJsonObject &request, QString *error);

private:
    ofi_job_manager *m_manager = nullptr;
    QString m_error;
};
