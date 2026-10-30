#include "models.h"

#include <QDateTime>
#include <QLocale>
#include <QTimeZone>

namespace {

QString humanBytes(quint64 bytes) {
    static const char *units[] = {"B", "KiB", "MiB", "GiB", "TiB"};
    double value = static_cast<double>(bytes);
    int unit = 0;
    while (value >= 1024.0 && unit < 4) {
        value /= 1024.0;
        unit++;
    }
    return unit == 0
        ? QString::number(bytes) + QStringLiteral(" B")
        : QString::number(value, 'f', 1) + QLatin1Char(' ') + units[unit];
}

QString objectType(quint32 metadataType) {
    switch (metadataType) {
    case 1: return QStringLiteral("File");
    case 2: return QStringLiteral("Directory");
    case 3: return QStringLiteral("FIFO");
    case 4: return QStringLiteral("Character device");
    case 5: return QStringLiteral("Block device");
    case 6: return QStringLiteral("Symlink");
    case 8: return QStringLiteral("Socket");
    default: return QStringLiteral("Unknown");
    }
}

QString timestampValue(const QJsonValue &value) {
    if (!value.isObject()) {
        return QStringLiteral("Unavailable");
    }
    const QJsonObject object = value.toObject();
    const qint64 seconds = object.value(QStringLiteral("seconds_since_epoch")).toVariant().toLongLong();
    const QString semantics = object.value(QStringLiteral("semantics")).toString();
    const QDateTime utc = QDateTime::fromSecsSinceEpoch(seconds, QTimeZone::UTC);
    if (!utc.isValid()) {
        return QStringLiteral("Out of range (%1)").arg(semantics);
    }
    const QString rendered = utc.toString(Qt::ISODateWithMs);
    if (semantics == QStringLiteral("utc")) {
        return rendered + QStringLiteral(" UTC");
    }
    return rendered + QStringLiteral(" (%1)").arg(semantics);
}

} // namespace

EvidenceTreeModel::EvidenceTreeModel(QObject *parent) : QAbstractListModel(parent) {}

int EvidenceTreeModel::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : m_visible.size();
}

QVariant EvidenceTreeModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || index.row() < 0 || index.row() >= m_visible.size()) {
        return {};
    }
    const Node &node = m_visible.at(index.row());
    switch (role) {
    case NameRole: return node.name;
    case KindRole: return node.kind;
    case DepthRole: return node.depth;
    case ExpandableRole: return node.expandable;
    case ExpandedRole: return node.expanded;
    case NodeIdRole: return node.id;
    default: return {};
    }
}

QHash<int, QByteArray> EvidenceTreeModel::roleNames() const {
    return {
        {NameRole, "name"},
        {KindRole, "kind"},
        {DepthRole, "depth"},
        {ExpandableRole, "expandable"},
        {ExpandedRole, "expanded"},
        {NodeIdRole, "nodeId"},
    };
}

int EvidenceTreeModel::evidenceCount() const {
    return static_cast<int>(std::count_if(m_nodes.begin(), m_nodes.end(),
                                          [](const Node &node) { return node.image; }));
}

QVariantMap EvidenceTreeModel::nodeAt(int row) const {
    if (row < 0 || row >= m_visible.size()) {
        return {};
    }
    const Node &node = m_visible.at(row);
    return {
        {QStringLiteral("id"), node.id},
        {QStringLiteral("name"), node.name},
        {QStringLiteral("kind"), node.kind},
        {QStringLiteral("image"), node.image},
        {QStringLiteral("partition"), node.partition},
        {QStringLiteral("path"), node.path},
        {QStringLiteral("offset"), QVariant::fromValue(node.offset)},
        {QStringLiteral("payload"), node.payload.toVariantMap()},
    };
}

void EvidenceTreeModel::toggle(int row) {
    if (row < 0 || row >= m_visible.size()) {
        return;
    }
    const QString id = m_visible.at(row).id;
    const int stored = indexOf(id);
    if (stored < 0) {
        return;
    }
    m_nodes[stored].expanded = !m_nodes[stored].expanded;
    rebuild();
}

void EvidenceTreeModel::clear() {
    beginResetModel();
    m_nodes.clear();
    m_visible.clear();
    endResetModel();
    emit countChanged();
}

void EvidenceTreeModel::setImages(const QJsonArray &images) {
    beginResetModel();
    m_nodes.clear();
    for (const QJsonValue &value : images) {
        const QJsonObject object = value.toObject();
        Node node;
        node.id = object.value(QStringLiteral("id")).toString();
        node.name = object.value(QStringLiteral("name")).toString();
        node.kind = object.value(QStringLiteral("format")).toString().toUpper();
        node.depth = 0;
        node.expandable = true;
        node.expanded = true;
        node.image = true;
        node.path = object.value(QStringLiteral("path")).toString();
        node.payload = object;
        m_nodes.append(node);
    }
    endResetModel();
    rebuild();
    emit countChanged();
}

void EvidenceTreeModel::setPartitions(const QString &imageId, const QJsonArray &partitions) {
    QList<Node> retained;
    for (const Node &node : std::as_const(m_nodes)) {
        if (!(node.parentId == imageId && node.partition)) {
            retained.append(node);
        }
    }
    const int imageIndex = indexOf(imageId);
    const QString imagePath = imageIndex >= 0 ? m_nodes.at(imageIndex).path : QString();
    for (const QJsonValue &value : partitions) {
        const QJsonObject object = value.toObject();
        if (object.value(QStringLiteral("allocated")).toBool() == false) {
            continue;
        }
        Node node;
        node.id = imageId + QStringLiteral(":partition:")
            + QString::number(object.value(QStringLiteral("address")).toVariant().toULongLong());
        node.parentId = imageId;
        node.name = object.value(QStringLiteral("description")).toString();
        if (node.name.isEmpty()) {
            node.name = QStringLiteral("Partition");
        }
        node.kind = humanBytes(object.value(QStringLiteral("length_bytes")).toVariant().toULongLong());
        node.depth = 1;
        node.expandable = true;
        node.partition = true;
        node.path = imagePath;
        node.offset = object.value(QStringLiteral("offset_bytes")).toVariant().toULongLong();
        node.payload = object;
        retained.append(node);
    }
    beginResetModel();
    m_nodes = retained;
    endResetModel();
    rebuild();
}

void EvidenceTreeModel::setFilesystem(const QString &imageId, const QJsonObject &filesystem) {
    QList<Node> retained;
    for (const Node &node : std::as_const(m_nodes)) {
        if (node.id != imageId + QStringLiteral(":filesystem")) {
            retained.append(node);
        }
    }
    Node node;
    node.id = imageId + QStringLiteral(":filesystem");
    node.parentId = imageId;
    node.name = QStringLiteral("Filesystem");
    node.kind = filesystem.value(QStringLiteral("kind")).toString();
    node.depth = 1;
    node.path = filesystem.value(QStringLiteral("path")).toString();
    node.offset = filesystem.value(QStringLiteral("offset_bytes")).toVariant().toULongLong();
    node.payload = filesystem;
    retained.append(node);
    beginResetModel();
    m_nodes = retained;
    endResetModel();
    rebuild();
}

void EvidenceTreeModel::rebuild() {
    beginResetModel();
    m_visible.clear();
    for (const Node &node : std::as_const(m_nodes)) {
        bool parentVisible = true;
        if (!node.parentId.isEmpty()) {
            const int parentIndex = indexOf(node.parentId);
            parentVisible = parentIndex >= 0 && m_nodes.at(parentIndex).expanded;
        }
        if (parentVisible) {
            m_visible.append(node);
        }
    }
    endResetModel();
}

int EvidenceTreeModel::indexOf(const QString &id) const {
    for (int i = 0; i < m_nodes.size(); ++i) {
        if (m_nodes.at(i).id == id) {
            return i;
        }
    }
    return -1;
}

FileListModel::FileListModel(QObject *parent) : QAbstractListModel(parent) {}

int FileListModel::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : m_entries.size();
}

QVariant FileListModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || index.row() < 0 || index.row() >= m_entries.size()) {
        return {};
    }
    const Entry &entry = m_entries.at(index.row());
    switch (role) {
    case NameRole: return entry.name;
    case SizeTextRole: return entry.sizeText;
    case TypeTextRole: return entry.typeText;
    case MetadataTextRole: return entry.metadataText;
    case StatusTextRole: return entry.statusText;
    case DeletedRole: return entry.deleted;
    case IsDirectoryRole: return entry.directory;
    default: return {};
    }
}

QHash<int, QByteArray> FileListModel::roleNames() const {
    return {
        {NameRole, "name"},
        {SizeTextRole, "sizeText"},
        {TypeTextRole, "typeText"},
        {MetadataTextRole, "metadataText"},
        {StatusTextRole, "statusText"},
        {DeletedRole, "deleted"},
        {IsDirectoryRole, "isDirectory"},
    };
}

QString FileListModel::locationLabel() const { return m_location; }
QString FileListModel::emptyMessage() const { return m_emptyMessage; }
bool FileListModel::loading() const { return m_loading; }

QVariantMap FileListModel::entryAt(int row) const {
    if (row < 0 || row >= m_entries.size()) {
        return {};
    }
    return m_entries.at(row).raw.toVariantMap();
}

void FileListModel::clear() {
    beginResetModel();
    m_entries.clear();
    endResetModel();
    m_loading = false;
    m_location.clear();
    m_emptyMessage = QStringLiteral("No directory selected.");
    emit locationChanged();
    emit stateChanged();
}

void FileListModel::setLoading(const QString &location) {
    beginResetModel();
    m_entries.clear();
    endResetModel();
    m_loading = true;
    m_location = location;
    m_emptyMessage = QStringLiteral("Loading directory entries.");
    emit locationChanged();
    emit stateChanged();
}

void FileListModel::setEntries(const QString &location, const QJsonArray &entries) {
    beginResetModel();
    m_entries.clear();
    for (const QJsonValue &value : entries) {
        const QJsonObject object = value.toObject();
        Entry entry;
        entry.raw = object;
        entry.name = object.value(QStringLiteral("name")).toString();
        entry.directory = object.value(QStringLiteral("is_directory")).toBool();
        entry.deleted = object.value(QStringLiteral("deleted")).toBool();
        const quint64 size = object.value(QStringLiteral("size")).toVariant().toULongLong();
        entry.sizeText = entry.directory ? QStringLiteral("—") : humanBytes(size);
        entry.typeText = objectType(
            object.value(QStringLiteral("metadata_type")).toVariant().toUInt());
        entry.metadataText = QString::number(
            object.value(QStringLiteral("metadata_address")).toVariant().toULongLong());
        entry.statusText = entry.deleted ? QStringLiteral("Deleted")
                                         : QStringLiteral("Allocated");
        m_entries.append(entry);
    }
    endResetModel();
    m_loading = false;
    m_location = location;
    m_emptyMessage = entries.isEmpty()
        ? QStringLiteral("This directory contains no entries.")
        : QString();
    emit locationChanged();
    emit stateChanged();
}

void FileListModel::setEmpty(const QString &message) {
    beginResetModel();
    m_entries.clear();
    endResetModel();
    m_loading = false;
    m_emptyMessage = message;
    emit stateChanged();
}

void FileListModel::setError(const QString &message) {
    beginResetModel();
    m_entries.clear();
    endResetModel();
    m_loading = false;
    m_emptyMessage = message;
    emit stateChanged();
}

PropertiesModel::PropertiesModel(QObject *parent) : QAbstractListModel(parent) {}

int PropertiesModel::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : m_rows.size();
}

QVariant PropertiesModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || index.row() < 0 || index.row() >= m_rows.size()) {
        return {};
    }
    const Row &row = m_rows.at(index.row());
    switch (role) {
    case GroupRole: return row.group;
    case GroupStartRole: return row.groupStart;
    case KeyRole: return row.key;
    case ValueRole: return row.value;
    case ProvenanceRole: return row.provenance;
    case MonospaceRole: return row.monospace;
    default: return {};
    }
}

QHash<int, QByteArray> PropertiesModel::roleNames() const {
    return {
        {GroupRole, "group"},
        {GroupStartRole, "groupStart"},
        {KeyRole, "key"},
        {ValueRole, "value"},
        {ProvenanceRole, "provenance"},
        {MonospaceRole, "monospace"},
    };
}

void PropertiesModel::clear() {
    beginResetModel();
    m_rows.clear();
    endResetModel();
}

void PropertiesModel::append(const QString &group, const QString &key,
                             const QString &value, const QString &provenance,
                             bool monospace) {
    Row row;
    row.group = group;
    row.groupStart = m_rows.isEmpty() || m_rows.last().group != group;
    row.key = key;
    row.value = value;
    row.provenance = provenance;
    row.monospace = monospace;
    beginInsertRows({}, m_rows.size(), m_rows.size());
    m_rows.append(row);
    endInsertRows();
}

void PropertiesModel::setFromImage(const QString &path) {
    clear();
    append(QStringLiteral("Source"), QStringLiteral("Path"), path, {}, true);
    append(QStringLiteral("Source"), QStringLiteral("Access"),
           QStringLiteral("Read-only"));
}

void PropertiesModel::setFromPartition(const QJsonObject &partition) {
    clear();
    append(QStringLiteral("Partition"), QStringLiteral("Description"),
           partition.value(QStringLiteral("description")).toString());
    append(QStringLiteral("Partition"), QStringLiteral("Start sector"),
           QString::number(partition.value(QStringLiteral("start_sector")).toVariant().toULongLong()),
           {}, true);
    append(QStringLiteral("Partition"), QStringLiteral("Sectors"),
           QString::number(partition.value(QStringLiteral("sector_count")).toVariant().toULongLong()),
           {}, true);
    append(QStringLiteral("Partition"), QStringLiteral("Byte offset"),
           humanBytes(partition.value(QStringLiteral("offset_bytes")).toVariant().toULongLong()),
           {}, true);
    append(QStringLiteral("Partition"), QStringLiteral("Length"),
           humanBytes(partition.value(QStringLiteral("length_bytes")).toVariant().toULongLong()));
}

void PropertiesModel::setFromEntry(const QJsonObject &entry) {
    clear();
    append(QStringLiteral("Name"), QStringLiteral("Name"),
           entry.value(QStringLiteral("name")).toString());
    append(QStringLiteral("Name"), QStringLiteral("Metadata address"),
           QString::number(entry.value(QStringLiteral("metadata_address")).toVariant().toULongLong()),
           {}, true);
    append(QStringLiteral("Name"), QStringLiteral("Parent address"),
           QString::number(entry.value(QStringLiteral("parent_address")).toVariant().toULongLong()),
           {}, true);
    append(QStringLiteral("Content"), QStringLiteral("Size"),
           humanBytes(entry.value(QStringLiteral("size")).toVariant().toULongLong()));
    append(QStringLiteral("Content"), QStringLiteral("Type"),
           objectType(entry.value(QStringLiteral("metadata_type")).toVariant().toUInt()));
    append(QStringLiteral("Content"), QStringLiteral("Status"),
           entry.value(QStringLiteral("deleted")).toBool()
               ? QStringLiteral("Deleted") : QStringLiteral("Allocated"));
    append(QStringLiteral("Timestamps"), QStringLiteral("Modified"),
           timestampValue(entry.value(QStringLiteral("mtime"))),
           QStringLiteral("Source semantics are preserved; no timezone is invented."));
    append(QStringLiteral("Timestamps"), QStringLiteral("Accessed"),
           timestampValue(entry.value(QStringLiteral("atime"))));
    append(QStringLiteral("Timestamps"), QStringLiteral("Changed"),
           timestampValue(entry.value(QStringLiteral("ctime"))));
    append(QStringLiteral("Timestamps"), QStringLiteral("Created"),
           timestampValue(entry.value(QStringLiteral("crtime"))));
}

TaskModel::TaskModel(QObject *parent) : QAbstractListModel(parent) {}

int TaskModel::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : m_rows.size();
}

QVariant TaskModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || index.row() < 0 || index.row() >= m_rows.size()) {
        return {};
    }
    const Row &row = m_rows.at(index.row());
    switch (role) {
    case JobIdRole: return row.jobId;
    case TitleRole: return row.title;
    case StatusRole: return row.status;
    case DetailRole: return row.detail;
    case ProgressRole: return row.progress;
    case CancellableRole: return row.cancellable;
    default: return {};
    }
}

QHash<int, QByteArray> TaskModel::roleNames() const {
    return {
        {JobIdRole, "jobId"},
        {TitleRole, "title"},
        {StatusRole, "status"},
        {DetailRole, "detail"},
        {ProgressRole, "progress"},
        {CancellableRole, "cancellable"},
    };
}

void TaskModel::addOrUpdate(const QString &jobId, const QString &title,
                            const QJsonObject &snapshot) {
    const QString status = snapshot.value(QStringLiteral("status")).toString();
    const quint64 completed =
        snapshot.value(QStringLiteral("completed_bytes")).toVariant().toULongLong();
    const quint64 total =
        snapshot.value(QStringLiteral("total_bytes")).toVariant().toULongLong();
    const QString detail = snapshot.value(QStringLiteral("message")).toString();
    const bool cancellable = status == QStringLiteral("pending")
        || status == QStringLiteral("running");
    const double progress = total == 0 ? 0.0
        : qBound(0.0, static_cast<double>(completed) / static_cast<double>(total), 1.0);

    for (int i = 0; i < m_rows.size(); ++i) {
        if (m_rows.at(i).jobId == jobId) {
            m_rows[i].status = status;
            m_rows[i].detail = detail;
            m_rows[i].progress = progress;
            m_rows[i].cancellable = cancellable;
            emit dataChanged(index(i), index(i));
            return;
        }
    }
    Row row;
    row.jobId = jobId;
    row.title = title;
    row.status = status;
    row.detail = detail;
    row.progress = progress;
    row.cancellable = cancellable;
    beginInsertRows({}, m_rows.size(), m_rows.size());
    m_rows.append(row);
    endInsertRows();
}

void TaskModel::removeCompleted() {
    for (int i = m_rows.size() - 1; i >= 0; --i) {
        const QString &status = m_rows.at(i).status;
        if (status == QStringLiteral("succeeded")
            || status == QStringLiteral("failed")
            || status == QStringLiteral("cancelled")
            || status == QStringLiteral("partial")) {
            beginRemoveRows({}, i, i);
            m_rows.removeAt(i);
            endRemoveRows();
        }
    }
}

ViewerModel::ViewerModel(QObject *parent) : QObject(parent) {}

void ViewerModel::setLoading(const QString &what) {
    m_loading = true;
    m_status = what;
    emit changed();
}

void ViewerModel::setHex(const QJsonObject &page) {
    m_hexRows.clear();
    const QJsonArray bytes = page.value(QStringLiteral("bytes")).toArray();
    const quint64 base = page.value(QStringLiteral("offset")).toVariant().toULongLong();
    for (int start = 0; start < bytes.size(); start += 16) {
        QString hex;
        QString ascii;
        for (int i = start; i < qMin(start + 16, bytes.size()); ++i) {
            const int value = bytes.at(i).toInt();
            if (!hex.isEmpty()) {
                hex += QLatin1Char(' ');
            }
            hex += QStringLiteral("%1").arg(value, 2, 16, QLatin1Char('0'));
            ascii += (value >= 32 && value < 127) ? QChar(value) : QLatin1Char('.');
        }
        m_hexRows.append(QVariantMap{
            {QStringLiteral("offset"),
             QStringLiteral("%1").arg(base + static_cast<quint64>(start), 8, 16, QLatin1Char('0'))},
            {QStringLiteral("hex"), hex},
            {QStringLiteral("ascii"), ascii},
        });
    }
    m_loading = false;
    m_status = page.value(QStringLiteral("truncated")).toBool()
        ? QStringLiteral("Preview is limited to the requested page.")
        : QStringLiteral("Hex page loaded.");
    emit changed();
}

void ViewerModel::setText(const QJsonObject &preview) {
    const QString content = preview.value(QStringLiteral("content")).toString();
    if (content == QStringLiteral("binary")) {
        m_text = QStringLiteral("Binary content is not rendered as text. Use the hex viewer.");
    } else {
        m_text = preview.value(QStringLiteral("text")).toString();
    }
    m_loading = false;
    emit changed();
}

void ViewerModel::setSignature(const QJsonObject &signature) {
    const QString kind = signature.value(QStringLiteral("kind")).toString();
    const QString category = signature.value(QStringLiteral("category")).toString();
    m_signature = kind.isEmpty()
        ? QStringLiteral("Unknown")
        : kind.toUpper() + QStringLiteral(" (") + category + QLatin1Char(')');
    emit changed();
}

void ViewerModel::setMetadata(const QString &metadata) {
    m_metadata = metadata;
    emit changed();
}

void ViewerModel::setError(const QString &message) {
    m_loading = false;
    m_status = message;
    emit changed();
}

void ViewerModel::clear() {
    m_hexRows.clear();
    m_text.clear();
    m_metadata.clear();
    m_signature.clear();
    m_status.clear();
    m_loading = false;
    emit changed();
}
