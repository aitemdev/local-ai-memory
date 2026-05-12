import { createHash } from "node:crypto";
import { getSettings } from "./settings.js";

export const DEFAULT_LOCAL_MODEL = "local-hash-v1";
export const DEFAULT_DIMENSIONS = 128;

export async function embedText(text, options = {}) {
  const config = resolveEmbeddingConfig(options);
  if (config.provider === "openai") {
    return embedOpenAICompatible(text, config, "OpenAI");
  }
  if (config.provider === "openrouter") {
    return embedOpenAICompatible(text, config, "OpenRouter");
  }
  if (config.provider === "ollama") {
    return embedOpenAICompatible(text, config, "Ollama");
  }
  return {
    model: DEFAULT_LOCAL_MODEL,
    provider: "local",
    dimensions: DEFAULT_DIMENSIONS,
    vector: localHashEmbedding(text, DEFAULT_DIMENSIONS)
  };
}

export function resolveEmbeddingConfig(options = {}) {
  const settings = options.skipSettings ? {} : getSettings([
    "embedding.provider",
    "embedding.default_model",
    "embedding.dimensions",
    "embedding.base_url"
  ], { base: options.base });

  const provider = (options.provider || process.env.MEM_EMBEDDING_PROVIDER || settings["embedding.provider"] || "local").toLowerCase();
  const model = options.model || process.env.MEM_EMBEDDING_MODEL || settings["embedding.default_model"] || defaultModelForProvider(provider);
  const dimensionsValue = options.dimensions || process.env.MEM_EMBEDDING_DIMENSIONS || settings["embedding.dimensions"];
  const dimensions = dimensionsValue ? Number(dimensionsValue) : undefined;
  const baseUrl = trimTrailingSlash(options.baseUrl || process.env.MEM_EMBEDDING_BASE_URL || settings["embedding.base_url"] || defaultBaseUrlForProvider(provider));
  const apiKey = options.apiKey || apiKeyForProvider(provider);

  if (provider !== "local" && !model) throw new Error(`Missing embedding model for provider ${provider}`);
  if (!options.allowMissingApiKey && provider !== "local" && !apiKey && provider !== "ollama") {
    throw new Error(`Missing API key for ${provider}. Set ${apiKeyEnvForProvider(provider)}.`);
  }

  return {
    provider,
    model: model || DEFAULT_LOCAL_MODEL,
    dimensions,
    baseUrl,
    apiKey
  };
}

export function localHashEmbedding(text, dimensions = DEFAULT_DIMENSIONS) {
  const vector = new Array(dimensions).fill(0);
  const words = text.toLowerCase().normalize("NFKD").replace(/[^\p{Letter}\p{Number}\s-]/gu, " ").split(/\s+/).filter(Boolean);
  for (const word of words) {
    const digest = createHash("sha256").update(word).digest();
    const index = digest.readUInt32BE(0) % dimensions;
    const sign = digest[4] % 2 === 0 ? 1 : -1;
    vector[index] += sign;
  }
  return normalize(vector);
}

export function cosineSimilarity(a, b) {
  let sum = 0;
  const length = Math.min(a.length, b.length);
  for (let i = 0; i < length; i++) sum += a[i] * b[i];
  return sum;
}

function normalize(vector) {
  const norm = Math.sqrt(vector.reduce((acc, value) => acc + value * value, 0)) || 1;
  return vector.map((value) => Number((value / norm).toFixed(6)));
}

async function embedOpenAICompatible(text, config, label) {
  const body = { model: config.model, input: text };
  if (config.dimensions) body.dimensions = config.dimensions;
  const headers = { "content-type": "application/json" };
  if (config.apiKey) headers.authorization = `Bearer ${config.apiKey}`;
  const response = await fetch(`${config.baseUrl}/embeddings`, {
    method: "POST",
    headers,
    body: JSON.stringify(body)
  });
  if (!response.ok) {
    throw new Error(`${label} embeddings failed: ${response.status} ${await response.text()}`);
  }
  const json = await response.json();
  const vector = json.data?.[0]?.embedding;
  if (!Array.isArray(vector)) throw new Error(`${label} embeddings response did not include a vector.`);
  return {
    provider: config.provider,
    model: json.model || config.model,
    dimensions: vector.length,
    vector
  };
}

export function defaultModelForProvider(provider) {
  if (provider === "openai") return "text-embedding-3-small";
  if (provider === "openrouter") return "openai/text-embedding-3-small";
  if (provider === "ollama") return "nomic-embed-text";
  return DEFAULT_LOCAL_MODEL;
}

export function defaultBaseUrlForProvider(provider) {
  if (provider === "openai") return "https://api.openai.com/v1";
  if (provider === "openrouter") return "https://openrouter.ai/api/v1";
  if (provider === "ollama") return "http://localhost:11434/v1";
  return "";
}

function apiKeyForProvider(provider) {
  if (provider === "openai") return process.env.OPENAI_API_KEY;
  if (provider === "openrouter") return process.env.OPENROUTER_API_KEY;
  if (provider === "ollama") return process.env.OLLAMA_API_KEY || "ollama";
  return undefined;
}

function apiKeyEnvForProvider(provider) {
  if (provider === "openai") return "OPENAI_API_KEY";
  if (provider === "openrouter") return "OPENROUTER_API_KEY";
  if (provider === "ollama") return "OLLAMA_API_KEY";
  return "";
}

function trimTrailingSlash(value) {
  return value ? value.replace(/\/+$/, "") : value;
}
