import { sha256Text } from "./hash.js";

export function chunkMarkdown(markdown, options = {}) {
  const targetTokens = options.targetTokens || 420;
  const maxTokens = options.maxTokens || 620;
  const sections = splitByHeadings(markdown);
  const chunks = [];
  let ordinal = 0;

  for (const section of sections) {
    const paragraphs = section.text.split(/\n{2,}/).map((p) => p.trim()).filter(Boolean);
    let buffer = [];
    let bufferTokens = 0;
    for (const paragraph of paragraphs) {
      const tokens = estimateTokens(paragraph);
      if (buffer.length && bufferTokens + tokens > maxTokens) {
        chunks.push(makeChunk(buffer.join("\n\n"), section.heading, ordinal++));
        buffer = [];
        bufferTokens = 0;
      }
      if (tokens > maxTokens) {
        for (const piece of splitLargeParagraph(paragraph, targetTokens)) {
          chunks.push(makeChunk(piece, section.heading, ordinal++));
        }
      } else {
        buffer.push(paragraph);
        bufferTokens += tokens;
      }
    }
    if (buffer.length) chunks.push(makeChunk(buffer.join("\n\n"), section.heading, ordinal++));
  }

  return chunks;
}

export function estimateTokens(text) {
  return Math.max(1, Math.ceil(text.length / 4));
}

function splitByHeadings(markdown) {
  const lines = markdown.split("\n");
  const sections = [];
  let current = { heading: undefined, text: "" };
  for (const line of lines) {
    const heading = /^(#{1,6})\s+(.+)$/.exec(line);
    if (heading && current.text.trim()) {
      sections.push(current);
      current = { heading: heading[2].trim(), text: `${line}\n` };
    } else {
      if (heading) current.heading = heading[2].trim();
      current.text += `${line}\n`;
    }
  }
  if (current.text.trim()) sections.push(current);
  return sections;
}

function splitLargeParagraph(text, targetTokens) {
  const words = text.split(/\s+/);
  const targetWords = Math.max(60, Math.floor(targetTokens * 0.75));
  const pieces = [];
  for (let i = 0; i < words.length; i += targetWords) {
    pieces.push(words.slice(i, i + targetWords).join(" "));
  }
  return pieces;
}

function makeChunk(text, heading, ordinal) {
  const clean = text.trim();
  return {
    id: sha256Text(`${ordinal}:${clean}`).slice(0, 24),
    ordinal,
    text: clean,
    heading,
    token_count: estimateTokens(clean),
    hash: sha256Text(clean)
  };
}
