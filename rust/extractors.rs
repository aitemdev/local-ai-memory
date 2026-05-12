use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, process::Command};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredDocument {
    pub title: String,
    #[serde(rename = "type")]
    pub doc_type: String,
    #[serde(default)]
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub kind: Option<String>,
    pub heading: Option<String>,
    pub text: Option<String>,
    pub page: Option<i64>,
    pub slide: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedDocument {
    pub parser: String,
    pub title: String,
    #[serde(rename = "type")]
    pub doc_type: String,
    pub markdown: String,
    pub structured: StructuredDocument,
}

pub fn supported_extension(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(),
        Some("md" | "txt" | "csv" | "tsv" | "json" | "html" | "htm" | "pdf" | "docx" | "pptx" | "xlsx" | "png" | "jpg" | "jpeg" | "tiff" | "bmp" | "webp")
    )
}

pub fn extract_document(path: &Path) -> Result<ExtractedDocument> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let title = path.file_name().and_then(|n| n.to_str()).unwrap_or("document").to_string();
    match ext.as_str() {
        "md" | "txt" | "csv" | "tsv" | "json" | "html" | "htm" => extract_text(path, &title, &ext),
        _ => extract_with_python(path),
    }
}

pub fn parser_status() -> serde_json::Value {
    let Some(python) = resolve_python() else {
        return serde_json::json!({ "python": null, "engines": {}, "ready": false });
    };
    let output = Command::new(&python)
        .arg("tools/parse_document.py")
        .arg("--probe")
        .output();
    match output {
        Ok(output) if output.status.success() => serde_json::from_slice(&output.stdout).unwrap_or_else(|_| serde_json::json!({ "python": python, "ready": false })),
        Ok(output) => serde_json::json!({ "python": python, "ready": false, "message": String::from_utf8_lossy(&output.stderr) }),
        Err(error) => serde_json::json!({ "python": python, "ready": false, "message": error.to_string() }),
    }
}

fn extract_text(path: &Path, title: &str, ext: &str) -> Result<ExtractedDocument> {
    let raw = fs::read_to_string(path)?;
    let markdown = normalize_text(&raw, ext, title);
    Ok(ExtractedDocument {
        parser: "native-rust-text".to_string(),
        title: title.to_string(),
        doc_type: ext.to_string(),
        structured: StructuredDocument {
            title: title.to_string(),
            doc_type: ext.to_string(),
            sections: vec![Section {
                kind: Some("document".to_string()),
                heading: Some(title.to_string()),
                text: Some(markdown.clone()),
                page: None,
                slide: None,
            }],
        },
        markdown,
    })
}

fn extract_with_python(path: &Path) -> Result<ExtractedDocument> {
    let python = resolve_python().ok_or_else(|| anyhow!("Python parser unavailable. Set MEM_PYTHON."))?;
    let output = Command::new(python)
        .arg("tools/parse_document.py")
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Err(anyhow!("{}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn resolve_python() -> Option<String> {
    let candidates = [
        std::env::var("MEM_PYTHON").ok(),
        Some("python3".to_string()),
        Some("python".to_string()),
        Some("py".to_string()),
    ];
    candidates.into_iter().flatten().find(|candidate| {
        Command::new(candidate)
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    })
}

fn normalize_text(raw: &str, ext: &str, title: &str) -> String {
    let trimmed = raw.replace("\r\n", "\n").trim().to_string();
    if ext == "md" {
        return trimmed;
    }
    format!("# {title}\n\n{trimmed}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_status_does_not_panic() {
        let status = parser_status();
        assert!(status.get("ready").is_some());
    }
}
