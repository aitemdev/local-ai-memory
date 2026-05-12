import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { embedText, resolveEmbeddingConfig } from "../src/embeddings.js";
import { initStore } from "../src/indexer.js";
import { setSettings } from "../src/settings.js";

test("resolves configured embedding provider from local settings", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mem-embedding-config-"));
  const store = path.join(dir, ".memoria");
  initStore(store);
  setSettings({
    "embedding.provider": "ollama",
    "embedding.default_model": "nomic-embed-text",
    "embedding.base_url": "http://localhost:11434/v1"
  }, { base: store });

  const config = resolveEmbeddingConfig({ base: store });
  assert.equal(config.provider, "ollama");
  assert.equal(config.model, "nomic-embed-text");
  assert.equal(config.baseUrl, "http://localhost:11434/v1");
});

test("calls OpenRouter embeddings endpoint with bearer token", async () => {
  const previousFetch = globalThis.fetch;
  const previousKey = process.env.OPENROUTER_API_KEY;
  process.env.OPENROUTER_API_KEY = "test-key";
  let request;
  globalThis.fetch = async (url, options) => {
    request = { url, options };
    return {
      ok: true,
      async json() {
        return { model: "openai/text-embedding-3-small", data: [{ embedding: [0.1, 0.2, 0.3] }] };
      }
    };
  };

  try {
    const result = await embedText("hello", {
      skipSettings: true,
      provider: "openrouter",
      model: "openai/text-embedding-3-small"
    });
    assert.equal(request.url, "https://openrouter.ai/api/v1/embeddings");
    assert.equal(request.options.headers.authorization, "Bearer test-key");
    assert.equal(result.provider, "openrouter");
    assert.equal(result.dimensions, 3);
  } finally {
    globalThis.fetch = previousFetch;
    if (previousKey === undefined) delete process.env.OPENROUTER_API_KEY;
    else process.env.OPENROUTER_API_KEY = previousKey;
  }
});
