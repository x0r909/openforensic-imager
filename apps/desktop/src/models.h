#pragma once

#include <QAbstractListModel>
#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QString>

class EvidenceTreeModel : public QAbstractListModel {
    Q_OBJECT
    Q_PROPERTY(int evidenceCount READ evidenceCount NOTIFY countChanged)

public:
    enum Role {
        NameRole = Qt::UserRole + 1,
        KindRole,
        DepthRole,
        ExpandableRole,
        ExpandedRole,
        NodeIdRole,
    };

    explicit EvidenceTreeModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    int evidenceCount() const;
    Q_INVOKABLE QVariantMap nodeAt(int row) const;
    Q_INVOKABLE void toggle(int row);
    Q_INVOKABLE void clear();

    void setImages(const QJsonArray &images);
    void setPartitions(const QString &imageId, const QJsonArray &partitions);
    void setFilesystem(const QString &imageId, const QJsonObject &filesystem);

signals:
    void countChanged();

private:
    struct Node {
        QString id;
        QString parentId;
        QString name;
        QString kind;
        int depth = 0;
        bool expandable = false;
        bool expanded = false;
        bool image = false;
        bool partition = false;
        QString path;
        quint64 offset = 0;
        QJsonObject payload;
    };

    void rebuild();
    int indexOf(const QString &id) const;

    QList<Node> m_nodes;
    QList<Node> m_visible;
};

class FileListModel : public QAbstractListModel {
    Q_OBJECT
    Q_PROPERTY(QString locationLabel READ locationLabel NOTIFY locationChanged)
    Q_PROPERTY(QString emptyMessage READ emptyMessage NOTIFY stateChanged)
    Q_PROPERTY(bool loading READ loading NOTIFY stateChanged)

public:
    enum Role {
        NameRole = Qt::UserRole + 1,
        SizeTextRole,
        TypeTextRole,
        MetadataTextRole,
        StatusTextRole,
        DeletedRole,
        IsDirectoryRole,
    };

    explicit FileListModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    QString locationLabel() const;
    QString emptyMessage() const;
    bool loading() const;

    Q_INVOKABLE QVariantMap entryAt(int row) const;
    Q_INVOKABLE void clear();
    void setLoading(const QString &location);
    void setEntries(const QString &location, const QJsonArray &entries);
    void setEmpty(const QString &message);
    void setError(const QString &message);

signals:
    void locationChanged();
    void stateChanged();

private:
    struct Entry {
        QJsonObject raw;
        QString name;
        QString sizeText;
        QString typeText;
        QString metadataText;
        QString statusText;
        bool deleted = false;
        bool directory = false;
    };

    QList<Entry> m_entries;
    QString m_location;
    QString m_emptyMessage = QStringLiteral("No directory selected.");
    bool m_loading = false;
};

class PropertiesModel : public QAbstractListModel {
    Q_OBJECT

public:
    enum Role {
        GroupRole = Qt::UserRole + 1,
        GroupStartRole,
        KeyRole,
        ValueRole,
        ProvenanceRole,
        MonospaceRole,
    };

    explicit PropertiesModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void clear();
    void append(const QString &group, const QString &key, const QString &value,
                const QString &provenance = {}, bool monospace = false);
    void setFromEntry(const QJsonObject &entry);
    void setFromPartition(const QJsonObject &partition);
    void setFromImage(const QString &path);

private:
    struct Row {
        QString group;
        bool groupStart = false;
        QString key;
        QString value;
        QString provenance;
        bool monospace = false;
    };

    QList<Row> m_rows;
};

class TaskModel : public QAbstractListModel {
    Q_OBJECT

public:
    enum Role {
        JobIdRole = Qt::UserRole + 1,
        TitleRole,
        StatusRole,
        DetailRole,
        ProgressRole,
        CancellableRole,
    };

    explicit TaskModel(QObject *parent = nullptr);

    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void addOrUpdate(const QString &jobId, const QString &title,
                     const QJsonObject &snapshot);
    void removeCompleted();

private:
    struct Row {
        QString jobId;
        QString title;
        QString status;
        QString detail;
        double progress = 0.0;
        bool cancellable = false;
    };

    QList<Row> m_rows;
};

class ViewerModel : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList hexRows READ hexRows NOTIFY changed)
    Q_PROPERTY(QString text READ text NOTIFY changed)
    Q_PROPERTY(QString metadata READ metadata NOTIFY changed)
    Q_PROPERTY(QString signature READ signature NOTIFY changed)
    Q_PROPERTY(QString status READ status NOTIFY changed)
    Q_PROPERTY(bool loading READ loading NOTIFY changed)

public:
    explicit ViewerModel(QObject *parent = nullptr);

    QVariantList hexRows() const { return m_hexRows; }
    QString text() const { return m_text; }
    QString metadata() const { return m_metadata; }
    QString signature() const { return m_signature; }
    QString status() const { return m_status; }
    bool loading() const { return m_loading; }

    void setLoading(const QString &what);
    void setHex(const QJsonObject &page);
    void setText(const QJsonObject &preview);
    void setSignature(const QJsonObject &signature);
    void setMetadata(const QString &metadata);
    void setError(const QString &message);
    void clear();

signals:
    void changed();

private:
    QVariantList m_hexRows;
    QString m_text;
    QString m_metadata;
    QString m_signature;
    QString m_status;
    bool m_loading = false;
};
