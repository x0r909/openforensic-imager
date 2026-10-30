use extraction::ExtractionManifest;
use reporting::write_extraction_report_set;
use serde_json::json;
use tempfile::tempdir;

fn sample_manifest() -> ExtractionManifest {
    serde_json::from_value(json!({
        "schema_version": 1,
        "generated_at": "2026-10-30T00:00:00Z",
        "output_root": "/tmp/openforensic-extraction",
        "directory": "files",
        "status": "partial",
        "records": [{
            "original_name": "<script>alert(1)</script>.txt",
            "relative_path": "=formula.csv",
            "entry": {
                "name": "<script>alert(1)</script>.txt",
                "name_truncated": false,
                "metadata_address": 42,
                "parent_address": 2,
                "size": 16,
                "name_flags": 0,
                "metadata_flags": 0,
                "name_type": 5,
                "metadata_type": 1,
                "mode": 420,
                "link_count": 1,
                "uid": 1000,
                "gid": 1000,
                "deleted": false,
                "is_directory": false,
                "atime": null,
                "mtime": null,
                "ctime": null,
                "crtime": null
            },
            "exported_size": 16,
            "source_sha256": "00",
            "output_sha256": "00",
            "extracted_at": "2026-10-30T00:00:01Z"
        }],
        "failures": [{
            "original_name": "oversized.bin",
            "relative_path": "files/oversized.bin",
            "message": "limit exceeded"
        }]
    }))
    .unwrap()
}

#[test]
fn writes_json_csv_and_safe_html_extraction_reports() {
    let directory = tempdir().unwrap();
    let manifest = sample_manifest();

    let paths = write_extraction_report_set(directory.path(), "phase4", &manifest).unwrap();

    let json = std::fs::read_to_string(&paths.json).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["extraction"]["status"], "partial");
    assert_eq!(parsed["extraction"]["records"][0]["output_sha256"], "00");

    let csv = std::fs::read_to_string(&paths.csv).unwrap();
    assert!(csv.contains("\"'=formula.csv\""));

    let html = std::fs::read_to_string(&paths.html).unwrap();
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;.txt"));
    assert!(!html.contains("<script>alert(1)</script>"));
    assert!(html.contains("limit exceeded"));
}
