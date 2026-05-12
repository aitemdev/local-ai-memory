use crate::{chunker::estimate_tokens, indexer::SearchResult};
use std::collections::HashSet;

pub fn rerank(query: &str, mut rows: Vec<SearchResult>) -> Vec<SearchResult> {
    let query_terms = tokenize(query);
    let query_phrase = normalize(query);
    let max_fts = rows.iter().map(|r| r.fts_score.max(0.0)).fold(0.000001, f32::max);
    let max_vector = rows.iter().map(|r| r.vector_score.max(0.0)).fold(0.000001, f32::max);

    for row in &mut rows {
        let haystack = normalize(&format!("{} {} {}", row.title, row.heading.clone().unwrap_or_default(), row.text));
        let terms: HashSet<_> = tokenize(&haystack).into_iter().collect();
        let overlap = if query_terms.is_empty() {
            0.0
        } else {
            query_terms.iter().filter(|term| terms.contains(*term)).count() as f32 / query_terms.len() as f32
        };
        let phrase = if query_phrase.len() >= 4 && haystack.contains(&query_phrase) { 1.0 } else { 0.0 };
        let lexical = (row.fts_score.max(0.0) / max_fts).min(1.0);
        let semantic = (row.vector_score.max(0.0) / max_vector).min(1.0);
        let compactness = (520.0 / estimate_tokens(&row.text).max(1) as f32).min(1.0);
        row.score = round(semantic * 0.32 + lexical * 0.24 + overlap * 0.18 + phrase * 0.14 + compactness * 0.03);
        row.score_breakdown = serde_json::json!({
            "semantic": round(semantic),
            "lexical": round(lexical),
            "overlap": round(overlap),
            "phrase": phrase,
            "compactness": round(compactness)
        });
    }
    rows.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    rows
}

pub fn apply_budget(rows: Vec<SearchResult>, budget: &str, limit: Option<usize>) -> Vec<SearchResult> {
    let max_results = limit.unwrap_or_else(|| match budget {
        "low" => 5,
        "wide" | "amplio" => 20,
        _ => 10,
    });
    let max_tokens = match budget {
        "low" => 1800,
        "wide" | "amplio" => 9000,
        _ => 4200,
    };
    let mut selected = Vec::new();
    let mut used = 0usize;
    for row in rows {
        if selected.len() >= max_results {
            break;
        }
        if !selected.is_empty() && used + row.token_count > max_tokens {
            continue;
        }
        used += row.token_count;
        selected.push(row);
    }
    selected
}

fn tokenize(value: &str) -> Vec<String> {
    normalize(value)
        .split_whitespace()
        .filter(|term| term.len() > 2)
        .map(ToString::to_string)
        .collect()
}

fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c.is_whitespace() || c == '-' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn round(value: f32) -> f32 {
    (value * 10000.0).round() / 10000.0
}
