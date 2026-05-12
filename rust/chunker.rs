use crate::hash::hash_text;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Chunk {
    pub id: String,
    pub ordinal: usize,
    pub text: String,
    pub heading: Option<String>,
    pub page: Option<i64>,
    pub slide: Option<i64>,
    pub token_count: usize,
    pub hash: String,
}

pub fn estimate_tokens(text: &str) -> usize {
    std::cmp::max(1, text.chars().count().div_ceil(4))
}

pub fn chunk_markdown(markdown: &str) -> Vec<Chunk> {
    let sections = split_by_headings(markdown);
    let mut chunks = Vec::new();
    let mut ordinal = 0usize;

    for (heading, text) in sections {
        let paragraphs: Vec<_> = text
            .split("\n\n")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        let mut buffer: Vec<String> = Vec::new();
        let mut buffer_tokens = 0usize;
        for paragraph in paragraphs {
            let tokens = estimate_tokens(paragraph);
            if !buffer.is_empty() && buffer_tokens + tokens > 620 {
                chunks.push(make_chunk(&buffer.join("\n\n"), heading.clone(), ordinal));
                ordinal += 1;
                buffer.clear();
                buffer_tokens = 0;
            }
            buffer.push(paragraph.to_string());
            buffer_tokens += tokens;
        }
        if !buffer.is_empty() {
            chunks.push(make_chunk(&buffer.join("\n\n"), heading, ordinal));
            ordinal += 1;
        }
    }
    chunks
}

fn split_by_headings(markdown: &str) -> Vec<(Option<String>, String)> {
    let mut sections = Vec::new();
    let mut heading = None;
    let mut text = String::new();
    for line in markdown.lines() {
        if line.starts_with('#') && line.contains(' ') && !text.trim().is_empty() {
            sections.push((heading.clone(), text.clone()));
            heading = line.split_once(' ').map(|(_, h)| h.trim().to_string());
            text.clear();
        } else if line.starts_with('#') && line.contains(' ') {
            heading = line.split_once(' ').map(|(_, h)| h.trim().to_string());
        }
        text.push_str(line);
        text.push('\n');
    }
    if !text.trim().is_empty() {
        sections.push((heading, text));
    }
    sections
}

fn make_chunk(text: &str, heading: Option<String>, ordinal: usize) -> Chunk {
    let clean = text.trim().to_string();
    Chunk {
        id: hash_text(&format!("{ordinal}:{clean}"))[..24].to_string(),
        ordinal,
        token_count: estimate_tokens(&clean),
        hash: hash_text(&clean),
        text: clean,
        heading,
        page: None,
        slide: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_markdown_by_headings_and_preserves_text() {
        let chunks = chunk_markdown("# Title\n\nAlpha beta gamma.\n\n## Next\n\nDelta epsilon.");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].heading.as_deref(), Some("Title"));
        assert!(chunks[1].text.contains("Delta"));
    }
}
