use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use acquisition::{AcquisitionManifest, VerificationReport};
use extraction::ExtractionManifest;
use forensic_core::UtcTimestamp;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReportPaths {
    pub json: PathBuf,
    pub csv: PathBuf,
    pub html: PathBuf,
}

#[derive(Debug)]
pub struct ReportError {
    message: String,
}

impl ReportError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for ReportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReportError {}

#[derive(Serialize)]
struct ReportBundle<'a> {
    schema_version: u32,
    generated_at: UtcTimestamp,
    acquisition: &'a AcquisitionManifest,
    verification: &'a VerificationReport,
}

pub fn write_report_set(
    output_dir: &Path,
    base_name: &str,
    acquisition: &AcquisitionManifest,
    verification: &VerificationReport,
) -> Result<ReportPaths, ReportError> {
    validate_output(output_dir, base_name)?;
    let bundle = ReportBundle {
        schema_version: 1,
        generated_at: UtcTimestamp::now(),
        acquisition,
        verification,
    };
    let json = serde_json::to_vec_pretty(&bundle)
        .map_err(|error| ReportError::new(format!("JSON report failed: {error}")))?;
    let csv = render_csv(acquisition, verification);
    let html = render_html(acquisition, verification);
    let paths = ReportPaths {
        json: output_dir.join(format!("{base_name}.report.json")),
        csv: output_dir.join(format!("{base_name}.report.csv")),
        html: output_dir.join(format!("{base_name}.report.html")),
    };
    write_owner_only(&paths.json, &json)?;
    write_owner_only(&paths.csv, csv.as_bytes())?;
    write_owner_only(&paths.html, html.as_bytes())?;
    Ok(paths)
}

pub fn write_extraction_report_set(
    output_dir: &Path,
    base_name: &str,
    extraction: &ExtractionManifest,
) -> Result<ReportPaths, ReportError> {
    validate_output(output_dir, base_name)?;
    let bundle = ExtractionReportBundle {
        schema_version: 1,
        generated_at: UtcTimestamp::now(),
        extraction,
    };
    let json = serde_json::to_vec_pretty(&bundle)
        .map_err(|error| ReportError::new(format!("JSON report failed: {error}")))?;
    let csv = render_extraction_csv(extraction);
    let html = render_extraction_html(extraction);
    let paths = ReportPaths {
        json: output_dir.join(format!("{base_name}.extraction.json")),
        csv: output_dir.join(format!("{base_name}.extraction.csv")),
        html: output_dir.join(format!("{base_name}.extraction.html")),
    };
    write_owner_only(&paths.json, &json)?;
    write_owner_only(&paths.csv, csv.as_bytes())?;
    write_owner_only(&paths.html, html.as_bytes())?;
    Ok(paths)
}

#[derive(Serialize)]
struct ExtractionReportBundle<'a> {
    schema_version: u32,
    generated_at: UtcTimestamp,
    extraction: &'a ExtractionManifest,
}

fn render_extraction_csv(extraction: &ExtractionManifest) -> String {
    let mut output = String::from("section,field,value\n");
    csv_row(
        &mut output,
        "manifest",
        "status",
        &format!("{:?}", extraction.status).to_lowercase(),
    );
    csv_row(
        &mut output,
        "manifest",
        "generated_at",
        &extraction.generated_at.to_rfc3339(),
    );
    csv_row(
        &mut output,
        "manifest",
        "output_root",
        &extraction.output_root.display().to_string(),
    );
    for record in &extraction.records {
        csv_row(
            &mut output,
            "record",
            "original_name",
            &record.original_name,
        );
        csv_row(
            &mut output,
            "record",
            "relative_path",
            &record.relative_path,
        );
        csv_row(
            &mut output,
            "record",
            "metadata_address",
            &record.entry.metadata_address.to_string(),
        );
        csv_row(
            &mut output,
            "record",
            "size",
            &record.exported_size.to_string(),
        );
        csv_row(
            &mut output,
            "record",
            "source_sha256",
            &record.source_sha256,
        );
        csv_row(
            &mut output,
            "record",
            "output_sha256",
            &record.output_sha256,
        );
    }
    for failure in &extraction.failures {
        csv_row(
            &mut output,
            "failure",
            "original_name",
            &failure.original_name,
        );
        csv_row(
            &mut output,
            "failure",
            "relative_path",
            failure.relative_path.as_deref().unwrap_or(""),
        );
        csv_row(&mut output, "failure", "message", &failure.message);
    }
    output
}

fn render_extraction_html(extraction: &ExtractionManifest) -> String {
    let mut output = String::new();
    output.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    output.push_str("<title>OpenForensic Imager Extraction Report</title></head><body>");
    output.push_str("<h1>OpenForensic Imager Extraction Report</h1>");
    output.push_str("<table>");
    html_row(&mut output, "Status", &format!("{:?}", extraction.status));
    html_row(
        &mut output,
        "Generated at (UTC)",
        &extraction.generated_at.to_rfc3339(),
    );
    html_row(
        &mut output,
        "Output root",
        &extraction.output_root.display().to_string(),
    );
    output.push_str("</table><h2>Extracted files</h2><table>");
    output.push_str(
        "<tr><th>Original name</th><th>Relative path</th><th>Size</th><th>SHA-256</th></tr>",
    );
    for record in &extraction.records {
        let _ = write!(
            output,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&record.original_name),
            escape_html(&record.relative_path),
            record.exported_size,
            escape_html(&record.output_sha256)
        );
    }
    output.push_str("</table><h2>Failures</h2><table>");
    output.push_str("<tr><th>Original name</th><th>Path</th><th>Error</th></tr>");
    for failure in &extraction.failures {
        let _ = write!(
            output,
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&failure.original_name),
            escape_html(failure.relative_path.as_deref().unwrap_or("")),
            escape_html(&failure.message)
        );
    }
    output.push_str("</table></body></html>");
    output
}

fn render_csv(acquisition: &AcquisitionManifest, verification: &VerificationReport) -> String {
    let mut output = String::from("section,field,value\n");
    csv_row(
        &mut output,
        "acquisition",
        "job_id",
        &acquisition.job_id.to_string(),
    );
    csv_row(
        &mut output,
        "acquisition",
        "status",
        &format!("{:?}", acquisition.status).to_lowercase(),
    );
    csv_row(
        &mut output,
        "acquisition",
        "source_path",
        &acquisition.source_path,
    );
    csv_row(
        &mut output,
        "acquisition",
        "bytes_written",
        &acquisition.bytes_written.to_string(),
    );
    csv_row(
        &mut output,
        "acquisition",
        "unreadable_ranges",
        &acquisition.unreadable_ranges.len().to_string(),
    );
    for hash in &acquisition.hashes {
        csv_row(
            &mut output,
            "acquisition_hash",
            &format!("{:?}.{:?}", hash.algorithm, hash.scope).to_lowercase(),
            &hash.digest_hex,
        );
    }
    for artifact in &acquisition.artifacts {
        csv_row(&mut output, "artifact", &artifact.path, &artifact.sha256);
    }
    csv_row(
        &mut output,
        "verification",
        "path",
        &verification.path.display().to_string(),
    );
    for (algorithm, outcome) in &verification.outcomes {
        csv_row(
            &mut output,
            "verification_outcome",
            &format!("{algorithm:?}").to_lowercase(),
            &format!("{outcome:?}").to_lowercase(),
        );
    }
    output
}

fn render_html(acquisition: &AcquisitionManifest, verification: &VerificationReport) -> String {
    let mut output = String::new();
    output.push_str("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">");
    output.push_str("<title>OpenForensic Imager Acquisition Report</title></head><body>");
    output.push_str("<h1>OpenForensic Imager Acquisition Report</h1>");
    output.push_str("<h2>Acquisition</h2><table>");
    html_row(&mut output, "Job ID", &acquisition.job_id.to_string());
    html_row(&mut output, "Status", &format!("{:?}", acquisition.status));
    html_row(&mut output, "Source", &acquisition.source_path);
    html_row(
        &mut output,
        "Bytes written",
        &acquisition.bytes_written.to_string(),
    );
    html_row(
        &mut output,
        "Unreadable ranges",
        &acquisition.unreadable_ranges.len().to_string(),
    );
    html_row(&mut output, "Verified", &acquisition.verified.to_string());
    output.push_str("</table><h2>Hashes</h2><table>");
    for hash in &acquisition.hashes {
        html_row(
            &mut output,
            &format!("{:?} {:?}", hash.algorithm, hash.scope),
            &hash.digest_hex,
        );
    }
    output.push_str("</table><h2>Verification</h2><table>");
    html_row(
        &mut output,
        "Image",
        &verification.path.display().to_string(),
    );
    for (algorithm, outcome) in &verification.outcomes {
        html_row(
            &mut output,
            &format!("{algorithm:?}"),
            &format!("{outcome:?}"),
        );
    }
    output.push_str("</table></body></html>");
    output
}

fn csv_row(output: &mut String, section: &str, field: &str, value: &str) {
    let _ = writeln!(
        output,
        "{},{},{}",
        csv_cell(section),
        csv_cell(field),
        csv_cell(value)
    );
}

fn csv_cell(value: &str) -> String {
    let needs_formula_prefix = value
        .chars()
        .next()
        .is_some_and(|character| matches!(character, '=' | '+' | '-' | '@' | '\t' | '\r'));
    let safe = if needs_formula_prefix {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}

fn html_row(output: &mut String, label: &str, value: &str) {
    let _ = write!(
        output,
        "<tr><th>{}</th><td>{}</td></tr>",
        escape_html(label),
        escape_html(value)
    );
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn validate_output(output_dir: &Path, base_name: &str) -> Result<(), ReportError> {
    if !output_dir.is_dir() {
        return Err(ReportError::new("report output directory does not exist"));
    }
    if base_name.is_empty()
        || Path::new(base_name).components().count() != 1
        || base_name == "."
        || base_name == ".."
    {
        return Err(ReportError::new(
            "report base name must be a single path component",
        ));
    }
    Ok(())
}

fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), ReportError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| ReportError::new(error.to_string()))?;
    file.write_all(bytes)
        .map_err(|error| ReportError::new(error.to_string()))?;
    file.sync_all()
        .map_err(|error| ReportError::new(error.to_string()))
}
