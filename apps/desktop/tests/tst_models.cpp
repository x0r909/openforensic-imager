#include <QJsonArray>
#include <QJsonObject>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QUuid>
#include <QtTest>

#include "app_controller.h"
#include "models.h"

class ModelTests : public QObject {
    Q_OBJECT

private slots:
    void evidenceTreeStartsEmpty();
    void evidenceTreeHidesCollapsedChildren();
    void evidenceTreeShowsPartitionsAndFilesystem();
    void fileListFormatsEntries();
    void fileListMarksDeletedEntries();
    void propertiesExposeTimestampProvenance();
    void taskModelUpdatesProgress();
    void viewerRendersHexRows();
    void viewerRejectsBinaryText();
    void caseCreationWritesAuditLog();
};

void ModelTests::evidenceTreeStartsEmpty() {
    EvidenceTreeModel model;
    QCOMPARE(model.rowCount(), 0);
    QCOMPARE(model.evidenceCount(), 0);
}

void ModelTests::evidenceTreeHidesCollapsedChildren() {
    EvidenceTreeModel model;
    model.setImages({QJsonObject{{QStringLiteral("id"), QStringLiteral("image-1")},
                                 {QStringLiteral("name"), QStringLiteral("disk.raw")},
                                 {QStringLiteral("format"), QStringLiteral("raw")},
                                 {QStringLiteral("path"), QStringLiteral("/tmp/disk.raw")}}});
    model.setPartitions(
        QStringLiteral("image-1"),
        {QJsonObject{{QStringLiteral("address"), 1},
                     {QStringLiteral("description"), QStringLiteral("Linux")},
                     {QStringLiteral("start_sector"), 2048},
                     {QStringLiteral("sector_count"), 4096},
                     {QStringLiteral("offset_bytes"), 1048576},
                     {QStringLiteral("length_bytes"), 2097152},
                     {QStringLiteral("allocated"), true}}});
    QCOMPARE(model.rowCount(), 2);

    model.toggle(0);

    QCOMPARE(model.rowCount(), 1);
    QCOMPARE(model.data(model.index(0), EvidenceTreeModel::NameRole).toString(),
             QStringLiteral("disk.raw"));
}

void ModelTests::evidenceTreeShowsPartitionsAndFilesystem() {
    EvidenceTreeModel model;
    model.setImages({QJsonObject{{QStringLiteral("id"), QStringLiteral("image-1")},
                                 {QStringLiteral("name"), QStringLiteral("disk.raw")},
                                 {QStringLiteral("format"), QStringLiteral("raw")},
                                 {QStringLiteral("path"), QStringLiteral("/tmp/disk.raw")}}});
    model.setPartitions(
        QStringLiteral("image-1"),
        {QJsonObject{{QStringLiteral("address"), 1},
                     {QStringLiteral("description"), QStringLiteral("Linux")},
                     {QStringLiteral("offset_bytes"), 1048576},
                     {QStringLiteral("length_bytes"), 2097152},
                     {QStringLiteral("allocated"), true}}});
    model.setFilesystem(
        QStringLiteral("image-1"),
        QJsonObject{{QStringLiteral("kind"), QStringLiteral("ext4")},
                    {QStringLiteral("root_inode"), 2},
                    {QStringLiteral("path"), QStringLiteral("/tmp/disk.raw")},
                    {QStringLiteral("offset_bytes"), 1048576}});

    QStringList kinds;
    for (int row = 0; row < model.rowCount(); ++row) {
        kinds.append(model.data(model.index(row), EvidenceTreeModel::KindRole).toString());
    }
    QVERIFY(kinds.contains(QStringLiteral("RAW")));
    QVERIFY(kinds.contains(QStringLiteral("ext4")));
}

void ModelTests::fileListFormatsEntries() {
    FileListModel model;
    model.setEntries(
        QStringLiteral("/"),
        {QJsonObject{{QStringLiteral("name"), QStringLiteral("hello.txt")},
                     {QStringLiteral("metadata_address"), 42},
                     {QStringLiteral("size"), 16},
                     {QStringLiteral("metadata_type"), 1},
                     {QStringLiteral("is_directory"), false},
                     {QStringLiteral("deleted"), false}}});

    QCOMPARE(model.rowCount(), 1);
    QCOMPARE(model.data(model.index(0), FileListModel::NameRole).toString(),
             QStringLiteral("hello.txt"));
    QCOMPARE(model.data(model.index(0), FileListModel::TypeTextRole).toString(),
             QStringLiteral("File"));
    QCOMPARE(model.data(model.index(0), FileListModel::StatusTextRole).toString(),
             QStringLiteral("Allocated"));
}

void ModelTests::fileListMarksDeletedEntries() {
    FileListModel model;
    model.setEntries(
        QStringLiteral("/"),
        {QJsonObject{{QStringLiteral("name"), QStringLiteral("removed.txt")},
                     {QStringLiteral("metadata_address"), 43},
                     {QStringLiteral("size"), 8},
                     {QStringLiteral("metadata_type"), 1},
                     {QStringLiteral("is_directory"), false},
                     {QStringLiteral("deleted"), true}}});

    QCOMPARE(model.data(model.index(0), FileListModel::DeletedRole).toBool(), true);
    QCOMPARE(model.data(model.index(0), FileListModel::StatusTextRole).toString(),
             QStringLiteral("Deleted"));
}

void ModelTests::propertiesExposeTimestampProvenance() {
    PropertiesModel model;
    model.setFromEntry(
        QJsonObject{{QStringLiteral("name"), QStringLiteral("hello.txt")},
                    {QStringLiteral("metadata_address"), 42},
                    {QStringLiteral("parent_address"), 2},
                    {QStringLiteral("size"), 16},
                    {QStringLiteral("metadata_type"), 1},
                    {QStringLiteral("deleted"), false},
                    {QStringLiteral("mtime"),
                     QJsonObject{{QStringLiteral("seconds_since_epoch"), 1700000000},
                                 {QStringLiteral("nanoseconds"), 0},
                                 {QStringLiteral("semantics"), QStringLiteral("utc")}}}});

    QString provenance;
    for (int row = 0; row < model.rowCount(); ++row) {
        provenance += model.data(model.index(row), PropertiesModel::ProvenanceRole).toString();
    }
    QVERIFY(provenance.contains(QStringLiteral("semantics")));
}

void ModelTests::taskModelUpdatesProgress() {
    TaskModel model;
    model.addOrUpdate(QStringLiteral("job-1"), QStringLiteral("Acquire"),
                      QJsonObject{{QStringLiteral("status"), QStringLiteral("running")},
                                  {QStringLiteral("completed_bytes"), 50},
                                  {QStringLiteral("total_bytes"), 100}});
    QCOMPARE(model.rowCount(), 1);
    QCOMPARE(model.data(model.index(0), TaskModel::ProgressRole).toDouble(), 0.5);
    QCOMPARE(model.data(model.index(0), TaskModel::CancellableRole).toBool(), true);

    model.addOrUpdate(QStringLiteral("job-1"), QStringLiteral("Acquire"),
                      QJsonObject{{QStringLiteral("status"), QStringLiteral("succeeded")},
                                  {QStringLiteral("completed_bytes"), 100},
                                  {QStringLiteral("total_bytes"), 100}});
    QCOMPARE(model.data(model.index(0), TaskModel::CancellableRole).toBool(), false);
}

void ModelTests::viewerRendersHexRows() {
    ViewerModel model;
    model.setHex(QJsonObject{{QStringLiteral("offset"), 16},
                             {QStringLiteral("bytes"), QJsonArray{65, 66, 67}},
                             {QStringLiteral("truncated"), true}});

    QCOMPARE(model.hexRows().size(), 1);
    const QVariantMap row = model.hexRows().first().toMap();
    QCOMPARE(row.value(QStringLiteral("offset")).toString(), QStringLiteral("00000010"));
    QCOMPARE(row.value(QStringLiteral("ascii")).toString(), QStringLiteral("ABC"));
}

void ModelTests::viewerRejectsBinaryText() {
    ViewerModel model;
    model.setText(QJsonObject{{QStringLiteral("content"), QStringLiteral("binary")},
                              {QStringLiteral("text"), QString()}});

    QVERIFY(model.text().contains(QStringLiteral("Binary content")));
}

void ModelTests::caseCreationWritesAuditLog() {
    QTemporaryDir directory;
    QVERIFY(directory.isValid());
    AppController controller;
    QSignalSpy errors(&controller, &AppController::errorRaised);
    controller.setCaseDatabasePath(directory.filePath(QStringLiteral("case.db")));
    controller.setCaseId(QUuid::createUuid().toString(QUuid::WithoutBraces));

    controller.createCase();

    QCOMPARE(errors.count(), 0);
    QFile audit(directory.filePath(QStringLiteral("audit.jsonl")));
    QVERIFY(audit.open(QIODevice::ReadOnly));
    QVERIFY(audit.readAll().contains("case.created"));
}

QTEST_MAIN(ModelTests)
#include "tst_models.moc"
