import { estimateTokens } from "./chunker.js";

export function rerankResults(query, rows) {
  const queryTerms = tokenize(query);
  const queryPhrase = normalize(query);
  const maxFts = Math.max(0.000001, ...rows.map((row) => Math.max(0, row.fts_score || 0)));
  const maxVector = Math.max(0.000001, ...rows.map((row) => Math.max(0, row.vector_score || 0)));

  return rows
    .map((row) => {
      const text = `${row.title || ""} ${row.heading || ""} ${row.text || ""}`;
      const textNorm = normalize(text);
      const textTerms = new Set(tokenize(text));
      const overlap = queryTerms.length
        ? queryTerms.filter((term) => textTerms.has(term)).length / queryTerms.length
        : 0;
      const exactPhrase = queryPhrase.length >= 4 && textNorm.includes(queryPhrase) ? 1 : 0;
      const headingBoost = row.heading && normalize(row.heading).includes(queryTerms[0] || "") ? 0.15 : 0;
      const density = termDensity(queryTerms, textNorm);
      const compactness = Math.min(1, 520 / Math.max(estimateTokens(row.text || ""), 1));

      const score_breakdown = {
        lexical: round(Math.max(0, row.fts_score || 0) / maxFts),
        semantic: round(Math.max(0, row.vector_score || 0) / maxVector),
        overlap: round(overlap),
        phrase: exactPhrase,
        density: round(density),
        heading: round(headingBoost),
        compactness: round(compactness)
      };

      const score = round(
        score_breakdown.semantic * 0.32 +
        score_breakdown.lexical * 0.24 +
        score_breakdown.overlap * 0.18 +
        score_breakdown.phrase * 0.14 +
        score_breakdown.density * 0.07 +
        score_breakdown.compactness * 0.03 +
        score_breakdown.heading * 0.02
      );

      return { ...row, score, score_breakdown };
    })
    .sort((a, b) => b.score - a.score);
}

export function applyTokenBudget(rows, budget, explicitLimit) {
  const maxResults = explicitLimit || budgetToLimit(budget);
  const maxTokens = budgetToTokenLimit(budget);
  const selected = [];
  let usedTokens = 0;

  for (const row of rows) {
    const tokens = row.token_count || estimateTokens(row.text || "");
    if (selected.length >= maxResults) break;
    if (selected.length > 0 && usedTokens + tokens > maxTokens) continue;
    selected.push({ ...row, token_count: tokens });
    usedTokens += tokens;
  }
  return selected;
}

export function budgetToLimit(budget = "normal") {
  if (budget === "low") return 5;
  if (budget === "wide" || budget === "amplio") return 20;
  return 10;
}

function budgetToTokenLimit(budget = "normal") {
  if (budget === "low") return 1800;
  if (budget === "wide" || budget === "amplio") return 9000;
  return 4200;
}

function tokenize(value) {
  return normalize(value)
    .split(/\s+/)
    .filter((term) => term.length > 2)
    .filter((term) => !stopwords.has(term));
}

function normalize(value) {
  return String(value || "")
    .toLowerCase()
    .normalize("NFKD")
    .replace(/\p{Diacritic}/gu, "")
    .replace(/[^\p{Letter}\p{Number}\s-]/gu, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function termDensity(queryTerms, textNorm) {
  if (!queryTerms.length || !textNorm) return 0;
  const words = textNorm.split(/\s+/);
  const hits = words.filter((word) => queryTerms.includes(word)).length;
  return Math.min(1, hits / Math.max(6, words.length / 18));
}

function round(value) {
  return Number(value.toFixed(4));
}

const stopwords = new Set([
  "the", "and", "for", "con", "que", "los", "las", "una", "uno", "del", "para",
  "por", "como", "sobre", "este", "esta", "that", "this", "from", "with"
]);
