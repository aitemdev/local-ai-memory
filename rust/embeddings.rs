use crate::settings::get_settings;
use anyhow::{Result, anyhow};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf};

pub const DEFAULT_LOCAL_MODEL: &str = "local-hash-v1";
pub const DEFAULT_DIMENSIONS: usize = 128;

#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub model: String,
    pub dimensions: Option<usize>,
    pub base_url: String,
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Embedding {
    pub provider: String,
    pub model: String,
    pub dimensions: usize,
    pub vector: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    model: Option<String>,
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

pub fn resolve_config(base: Option<PathBuf>, overrides: &HashMap<String, String>, allow_missing_key: bool) -> Result<EmbeddingConfig> {
    let settings = get_settings(
        &[
            "embedding.provider",
            "embedding.default_model",
            "embedding.dimensions",
            "embedding.base_url",
        ],
        base,
    )?;
    let provider = overrides
        .get("provider")
        .cloned()
        .or_else(|| std::env::var("MEM_EMBEDDING_PROVIDER").ok())
        .or_else(|| settings.get("embedding.provider").cloned())
        .unwrap_or_else(|| "local".to_string())
        .to_lowercase();
    let model = overrides
        .get("model")
        .cloned()
        .or_else(|| std::env::var("MEM_EMBEDDING_MODEL").ok())
        .or_else(|| settings.get("embedding.default_model").cloned())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| default_model(&provider).to_string());
    let dimensions = overrides
        .get("dimensions")
        .cloned()
        .or_else(|| std::env::var("MEM_EMBEDDING_DIMENSIONS").ok())
        .or_else(|| settings.get("embedding.dimensions").cloned())
        .filter(|d| !d.is_empty())
        .and_then(|d| d.parse().ok());
    let base_url = overrides
        .get("base_url")
        .cloned()
        .or_else(|| std::env::var("MEM_EMBEDDING_BASE_URL").ok())
        .or_else(|| settings.get("embedding.base_url").cloned())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| default_base_url(&provider).to_string())
        .trim_end_matches('/')
        .to_string();
    let api_key = api_key(&provider);
    if provider != "local" && provider != "ollama" && api_key.is_none() && !allow_missing_key {
        return Err(anyhow!("Missing API key for {provider}"));
    }
    Ok(EmbeddingConfig { provider, model, dimensions, base_url, api_key })
}

pub fn embed_text(text: &str, base: Option<PathBuf>, overrides: &HashMap<String, String>) -> Result<Embedding> {
    let config = resolve_config(base, overrides, false)?;
    match config.provider.as_str() {
        "local" => Ok(Embedding {
            provider: "local".to_string(),
            model: DEFAULT_LOCAL_MODEL.to_string(),
            dimensions: DEFAULT_DIMENSIONS,
            vector: local_hash_embedding(text, DEFAULT_DIMENSIONS),
        }),
        "openai" | "openrouter" | "ollama" => embed_openai_compatible(text, &config),
        other => Err(anyhow!("Unknown embedding provider: {other}")),
    }
}

pub fn embedding_key(embedding: &Embedding) -> String {
    format!("{}:{}", embedding.provider, embedding.model)
}

pub fn local_hash_embedding(text: &str, dimensions: usize) -> Vec<f32> {
    let mut vector = vec![0.0f32; dimensions];
    for word in text
        .to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .filter(|w| !w.is_empty())
    {
        let digest = Sha256::digest(word.as_bytes());
        let index = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) as usize % dimensions;
        let sign = if digest[4] % 2 == 0 { 1.0 } else { -1.0 };
        vector[index] += sign;
    }
    normalize(vector)
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

pub fn default_model(provider: &str) -> &'static str {
    match provider {
        "openai" => "text-embedding-3-small",
        "openrouter" => "openai/text-embedding-3-small",
        "ollama" => "nomic-embed-text",
        _ => DEFAULT_LOCAL_MODEL,
    }
}

pub fn default_base_url(provider: &str) -> &'static str {
    match provider {
        "openai" => "https://api.openai.com/v1",
        "openrouter" => "https://openrouter.ai/api/v1",
        "ollama" => "http://localhost:11434/v1",
        _ => "",
    }
}

fn embed_openai_compatible(text: &str, config: &EmbeddingConfig) -> Result<Embedding> {
    let mut body = serde_json::json!({ "model": config.model, "input": text });
    if let Some(dimensions) = config.dimensions {
        body["dimensions"] = serde_json::json!(dimensions);
    }
    let client = Client::new();
    let mut request = client
        .post(format!("{}/embeddings", config.base_url))
        .json(&body);
    if let Some(key) = &config.api_key {
        request = request.bearer_auth(key);
    }
    let response = request.send()?;
    if !response.status().is_success() {
        return Err(anyhow!("{} embeddings failed: {}", config.provider, response.text()?));
    }
    let json: EmbeddingResponse = response.json()?;
    let vector = json.data.first().ok_or_else(|| anyhow!("No embedding returned"))?.embedding.clone();
    Ok(Embedding {
        provider: config.provider.clone(),
        model: json.model.unwrap_or_else(|| config.model.clone()),
        dimensions: vector.len(),
        vector,
    })
}

fn normalize(mut vector: Vec<f32>) -> Vec<f32> {
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt().max(1.0);
    for value in &mut vector {
        *value /= norm;
    }
    vector
}

fn api_key(provider: &str) -> Option<String> {
    match provider {
        "openai" => std::env::var("OPENAI_API_KEY").ok(),
        "openrouter" => std::env::var("OPENROUTER_API_KEY").ok(),
        "ollama" => std::env::var("OLLAMA_API_KEY").ok().or_else(|| Some("ollama".to_string())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::set_settings;
    use tempfile::TempDir;

    #[test]
    #[ignore]
    fn ollama_embeddings_smoke() {
        if std::env::var("MEM_OLLAMA_TEST").is_err() {
            return;
        }
        let dir = TempDir::new().unwrap();
        let base = dir.path().join(".memoria");
        crate::indexer::init_store(Some(base.clone())).unwrap();
        let mut overrides = HashMap::new();
        overrides.insert("provider".to_string(), "ollama".to_string());
        overrides.insert("model".to_string(), "nomic-embed-text".to_string());
        overrides.insert("base_url".to_string(), "http://localhost:11434/v1".to_string());
        let embedding = embed_text("smoke test for ollama embeddings", Some(base), &overrides).unwrap();
        assert_eq!(embedding.provider, "ollama");
        assert!(embedding.dimensions >= 256);
        assert!(embedding.vector.iter().any(|v| v.abs() > 1e-6));
    }

    #[test]
    fn resolves_configured_provider_from_local_settings() {
        let dir = TempDir::new().unwrap();
        let base = dir.path().join(".memoria");
        crate::indexer::init_store(Some(base.clone())).unwrap();
        set_settings(
            &[
                ("embedding.provider", "ollama".to_string()),
                ("embedding.default_model", "nomic-embed-text".to_string()),
                ("embedding.base_url", "http://localhost:11434/v1".to_string()),
            ],
            Some(base.clone()),
        )
        .unwrap();
        let config = resolve_config(Some(base), &HashMap::new(), true).unwrap();
        assert_eq!(config.provider, "ollama");
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.base_url, "http://localhost:11434/v1");
    }
}
