import fs from "node:fs";
import path from "node:path";
import { randomUUID } from "node:crypto";
import { ensureStore } from "./db.js";
import { sha256File, sha256Text } from "./hash.js";
import { extractDocument, supportedExtensions } from "./extractors.js";
import { chunkMarkdown } from "./chunker.js";
import { cosineSimilarity, embedText } from "./embeddings.js";
import { applyTokenBudget, budgetToLimit, rerankResults } from "./reranker.js";

export function initStore(base) {
  const { db, paths } = ensureStore(base);
  db.close();
  return paths;
}

export async function addPath(inputPath, options = {}) {
  const resolved = path.resolve(inputPath);
  const stat = fs.statSync(resolved);
  const files = stat.isDirectory() ? walkSupportedFiles(resolved) : [resolved];
  const results = [];
  for (const file of files) results.push(await ingestFile(file, options));
  return results;
}

export async function ingestFile(filePath, options = {}) {
  const { db, paths } = ensureStore(options.base);
  const resolved = path.resolve(filePath);
  const jobId = randomUUID();
  const documentId = sha256Text(resolved).slice(0, 24);
  const hash = sha256File(resolved);
  const type = path.extname(resolved).slice(1).toLowerCase() || "unknown";
  const title = path.basename(resolved);

  db.prepare("INSERT INTO jobs (id, type, subject, status, progress) VALUES (?, 'ingest', ?, 'running', 5)").run(jobId, resolved);
  try {
    const existing = db.prepare("SELECT id, hash, status FROM documents WHERE path = ?").get(resolved);
    if (!options.force && existing?.hash === hash && existing.status === "ready") {
      db.prepare("UPDATE jobs SET status = 'completed', progress = 100, updated_at = CURRENT_TIMESTAMP WHERE id = ?").run(jobId);
      return { file: resolved, document_id: existing.id, status: "unchanged" };
    }

    const extracted = extractDocument(resolved);
    const canonicalMdPath = path.join(paths.canonical, `${documentId}.md`);
    const canonicalJsonPath = path.join(paths.canonical, `${documentId}.json`);
    fs.writeFileSync(canonicalMdPath, extracted.markdown, "utf8");
    fs.writeFileSync(canonicalJsonPath, JSON.stringify(extracted.structured, null, 2), "utf8");

    db.prepare(`
      INSERT INTO documents (id, path, hash, type, title, status, error, canonical_md_path, canonical_json_path, updated_at)
      VALUES (?, ?, ?, ?, ?, 'indexing', NULL, ?, ?, CURRENT_TIMESTAMP)
      ON CONFLICT(path) DO UPDATE SET
        hash = excluded.hash,
        type = excluded.type,
        title = excluded.title,
        status = 'indexing',
        error = NULL,
        canonical_md_path = excluded.canonical_md_path,
        canonical_json_path = excluded.canonical_json_path,
        updated_at = CURRENT_TIMESTAMP
    `).run(documentId, resolved, hash, extracted.type || type, extracted.title || title, canonicalMdPath, canonicalJsonPath);

    db.prepare("INSERT INTO document_versions (id, document_id, hash, parser, extraction_model) VALUES (?, ?, ?, ?, ?)").run(
      randomUUID(),
      documentId,
      hash,
      extracted.parser,
      "native"
    );

    db.prepare("DELETE FROM chunks_fts WHERE document_id = ?").run(documentId);
    db.prepare("DELETE FROM chunks WHERE document_id = ?").run(documentId);

    const chunks = chunksFromExtraction(extracted);
    const insertChunk = db.prepare(`
      INSERT OR IGNORE INTO chunks (id, document_id, ordinal, text, heading, page, slide, token_count, hash)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);
    const insertFts = db.prepare("INSERT INTO chunks_fts (chunk_id, document_id, title, text) VALUES (?, ?, ?, ?)");
    const insertEmbedding = db.prepare(`
      INSERT OR REPLACE INTO embeddings (chunk_id, model, dimensions, vector_json)
      VALUES (?, ?, ?, ?)
    `);

    for (const chunk of chunks) {
      const chunkId = sha256Text(`${documentId}:${chunk.hash}`).slice(0, 32);
      insertChunk.run(chunkId, documentId, chunk.ordinal, chunk.text, chunk.heading || null, chunk.page || null, chunk.slide || null, chunk.token_count, chunk.hash);
      insertFts.run(chunkId, documentId, extracted.title || title, chunk.text);
      const embedding = await embedText(chunk.text, { model: options.model, provider: options.provider, base: options.base });
      insertEmbedding.run(chunkId, embeddingKey(embedding), embedding.dimensions, JSON.stringify(embedding.vector));
    }

    db.prepare("UPDATE documents SET status = 'ready', error = NULL, updated_at = CURRENT_TIMESTAMP WHERE id = ?").run(documentId);
    db.prepare("UPDATE jobs SET status = 'completed', progress = 100, updated_at = CURRENT_TIMESTAMP WHERE id = ?").run(jobId);
    return { file: resolved, document_id: documentId, status: "ready", chunks: chunks.length };
  } catch (error) {
    db.prepare(`
      INSERT INTO documents (id, path, hash, type, title, status, error, updated_at)
      VALUES (?, ?, ?, ?, ?, 'error', ?, CURRENT_TIMESTAMP)
      ON CONFLICT(path) DO UPDATE SET hash = excluded.hash, status = 'error', error = excluded.error, updated_at = CURRENT_TIMESTAMP
    `).run(documentId, resolved, hash, type, title, error.message);
    db.prepare("UPDATE jobs SET status = 'failed', progress = 100, error = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?").run(error.message, jobId);
    return { file: resolved, document_id: documentId, status: "error", error: error.message };
  } finally {
    db.close();
  }
}

export async function searchMemory(query, options = {}) {
  const { db } = ensureStore(options.base);
  try {
    const limit = options.limit || budgetToLimit(options.budget);
    const candidateLimit = Math.max(40, limit * 8);
    const ftsRows = ftsSearch(db, query, candidateLimit);
    const vectorRows = await vectorSearch(db, query, candidateLimit, options);
    const merged = new Map();
    for (const row of ftsRows) {
      merged.set(row.chunk_id, { ...row, fts_score: row.score, vector_score: 0 });
    }
    for (const row of vectorRows) {
      const current = merged.get(row.chunk_id);
      if (current) current.vector_score = row.score;
      else merged.set(row.chunk_id, { ...row, fts_score: 0, vector_score: row.score });
    }
    const reranked = rerankResults(query, [...merged.values()]
      .map((row) => ({
        ...row,
        citation: formatCitation(row)
      })));
    return applyTokenBudget(reranked, options.budget, options.limit);
  } finally {
    db.close();
  }
}

export function status(options = {}) {
  const { db } = ensureStore(options.base);
  try {
    return {
      documents: plainRows(db.prepare("SELECT status, COUNT(*) AS count FROM documents GROUP BY status").all()),
      jobs: plainRows(db.prepare("SELECT status, COUNT(*) AS count FROM jobs GROUP BY status").all()),
      recent_jobs: plainRows(db.prepare("SELECT type, subject, status, progress, error, updated_at FROM jobs ORDER BY updated_at DESC LIMIT 8").all())
    };
  } finally {
    db.close();
  }
}

export function getDocument(documentId, options = {}) {
  const { db } = ensureStore(options.base);
  try {
    return plainRow(db.prepare("SELECT * FROM documents WHERE id = ?").get(documentId));
  } finally {
    db.close();
  }
}

export function getChunk(chunkId, options = {}) {
  const { db } = ensureStore(options.base);
  try {
    return plainRow(db.prepare(`
      SELECT c.*, d.title, d.path
      FROM chunks c
      JOIN documents d ON d.id = c.document_id
      WHERE c.id = ?
    `).get(chunkId));
  } finally {
    db.close();
  }
}

export function listCollections(options = {}) {
  const { db } = ensureStore(options.base);
  try {
    return plainRows(db.prepare("SELECT * FROM collections ORDER BY created_at DESC").all());
  } finally {
    db.close();
  }
}

function walkSupportedFiles(root) {
  const supported = new Set(supportedExtensions());
  const files = [];
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const full = path.join(root, entry.name);
    if (entry.isDirectory()) files.push(...walkSupportedFiles(full));
    else if (supported.has(path.extname(entry.name).toLowerCase())) files.push(full);
  }
  return files;
}

function ftsSearch(db, query, limit) {
  const ftsQuery = query.split(/\s+/).map((term) => term.replace(/["']/g, "")).filter(Boolean).map((term) => `"${term}"`).join(" OR ");
  if (!ftsQuery) return [];
  try {
    return db.prepare(`
      SELECT c.id AS chunk_id, c.document_id, c.text, c.heading, c.page, c.slide, d.title, d.path,
             c.ordinal, c.token_count,
             -bm25(chunks_fts) AS score
      FROM chunks_fts
      JOIN chunks c ON c.id = chunks_fts.chunk_id
      JOIN documents d ON d.id = c.document_id
      WHERE chunks_fts MATCH ?
      ORDER BY bm25(chunks_fts)
      LIMIT ?
    `).all(ftsQuery, limit);
  } catch {
    return [];
  }
}

async function vectorSearch(db, query, limit, options = {}) {
  const queryEmbedding = await embedText(query, options);
  const modelKey = embeddingKey(queryEmbedding);
  const rows = db.prepare(`
    SELECT c.id AS chunk_id, c.document_id, c.text, c.heading, c.page, c.slide, d.title, d.path,
           c.ordinal, c.token_count,
           e.vector_json
    FROM embeddings e
    JOIN chunks c ON c.id = e.chunk_id
    JOIN documents d ON d.id = c.document_id
    WHERE e.model = ?
  `).all(modelKey);
  return rows
    .map((row) => {
      const vector = JSON.parse(row.vector_json);
      return { ...row, vector_json: undefined, score: cosineSimilarity(queryEmbedding.vector, vector) };
    })
    .sort((a, b) => b.score - a.score)
    .slice(0, limit);
}

function embeddingKey(embedding) {
  return embedding.provider ? `${embedding.provider}:${embedding.model}` : embedding.model;
}

function formatCitation(row) {
  const location = row.page ? `page ${row.page}` : row.slide ? `slide ${row.slide}` : row.heading || "document";
  return `${row.title} (${location})`;
}

function chunksFromExtraction(extracted) {
  const sections = extracted.structured?.sections;
  if (!Array.isArray(sections) || sections.length === 0) return chunkMarkdown(extracted.markdown);

  const chunks = [];
  let ordinal = 0;
  for (const section of sections) {
    const heading = section.heading || extracted.title;
    const rawText = section.text || "";
    const sectionText = rawText.trim().startsWith("#") || !heading
      ? rawText
      : [`## ${heading}`, rawText].filter(Boolean).join("\n\n");
    for (const chunk of chunkMarkdown(sectionText)) {
      if (isHeadingOnlyChunk(chunk.text)) continue;
      chunks.push({
        ...chunk,
        id: sha256Text(`${ordinal}:${chunk.hash}`).slice(0, 24),
        ordinal,
        heading,
        page: section.page || null,
        slide: section.slide || null
      });
      ordinal += 1;
    }
  }
  return chunks.length ? chunks : chunkMarkdown(extracted.markdown);
}

function isHeadingOnlyChunk(text) {
  const lines = text.trim().split("\n").map((line) => line.trim()).filter(Boolean);
  return lines.length === 1 && /^#{1,6}\s+\S/.test(lines[0]);
}

function plainRow(row) {
  return row ? { ...row } : row;
}

function plainRows(rows) {
  return rows.map((row) => ({ ...row }));
}
