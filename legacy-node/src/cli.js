import fs from "node:fs";
import path from "node:path";
import { initStore, addPath, searchMemory, status, getChunk } from "./indexer.js";
import { serveMcp } from "./mcp.js";
import { serveHttp } from "./http.js";
import { memoryHome } from "./paths.js";
import { parserStatus } from "./extractors.js";
import { defaultModelForProvider, embedText, resolveEmbeddingConfig } from "./embeddings.js";
import { listSettings, setSettings } from "./settings.js";

export async function main(args) {
  const [command, ...rest] = args;
  if (!command || command === "help" || command === "--help") return printHelp();

  if (command === "init") {
    const paths = initStore();
    console.log(`Initialized local memory store at ${paths.base}`);
    return;
  }

  if (command === "add") {
    const { target, options } = parseAddArgs(rest);
    if (!target) throw new Error("Usage: mem add <file-or-folder>");
    const results = await addPath(target, options);
    for (const result of results) console.log(JSON.stringify(result));
    return;
  }

  if (command === "watch") {
    const target = rest[0];
    if (!target) throw new Error("Usage: mem watch <folder>");
    await watchFolder(target);
    return;
  }

  if (command === "search" || command === "ask") {
    const { query, options } = parseSearchArgs(rest);
    if (!query) throw new Error(`Usage: mem ${command} "<query>"`);
    const results = await searchMemory(query, {
      budget: options.budget || (command === "ask" ? "normal" : "low"),
      limit: options.limit
    });
    if (options.json) console.log(JSON.stringify(results, null, 2));
    else printResults(results, { debug: options.debug });
    return;
  }

  if (command === "open") {
    const chunkId = rest[0];
    if (!chunkId) throw new Error("Usage: mem open <chunk-id>");
    const chunk = getChunk(chunkId);
    if (!chunk) throw new Error(`Chunk not found: ${chunkId}`);
    console.log(chunk.path);
    return;
  }

  if (command === "status") {
    console.log(JSON.stringify(status(), null, 2));
    return;
  }

  if (command === "parsers") {
    console.log(JSON.stringify(parserStatus(), null, 2));
    return;
  }

  if (command === "embeddings") {
    await handleEmbeddings(rest);
    return;
  }

  if (command === "reindex") {
    const target = rest.find((arg) => !arg.startsWith("--")) || ".";
    const { options } = parseAddArgs(rest);
    const results = await addPath(target, { ...options, force: true });
    for (const result of results) console.log(JSON.stringify(result));
    return;
  }

  if (command === "serve") {
    if (rest.includes("--mcp")) return serveMcp();
    const portIndex = rest.indexOf("--port");
    const port = portIndex >= 0 ? Number(rest[portIndex + 1]) : 3737;
    return serveHttp({ port });
  }

  throw new Error(`Unknown command: ${command}`);
}

function printHelp() {
  console.log(`Local AI Memory

Usage:
  mem init
  mem add <file-or-folder> [--provider local|openai|openrouter|ollama] [--model model] [--force]
  mem watch <folder>
  mem search "<query>" [--budget low|normal|wide] [--limit n] [--json] [--debug]
  mem ask "<question>" [--budget low|normal|wide] [--limit n] [--json] [--debug]
  mem open <chunk-id>
  mem status
  mem parsers
  mem embeddings
  mem embeddings set --provider local|openai|openrouter|ollama --model <model> [--base-url url] [--dimensions n]
  mem embeddings test "hello"
  mem reindex <file-or-folder> [--provider provider] [--model model]
  mem serve --mcp
  mem serve --http --port 3737

Store: ${memoryHome()}
`);
}

function printResults(results, options = {}) {
  if (!results.length) {
    console.log("No results.");
    return;
  }
  for (const [index, result] of results.entries()) {
    const snippet = result.text.replace(/\s+/g, " ").slice(0, 260);
    console.log(`${index + 1}. ${result.citation} score=${result.score} tokens=${result.token_count} chunk=${result.chunk_id}`);
    console.log(`   ${snippet}${result.text.length > 260 ? "..." : ""}`);
    if (options.debug) {
      console.log(`   scores=${JSON.stringify(result.score_breakdown)} path=${result.path}`);
    }
  }
}

function parseSearchArgs(args) {
  const queryParts = [];
  const options = {};
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === "--budget") options.budget = args[++i];
    else if (arg === "--limit") options.limit = Number(args[++i]);
    else if (arg === "--json") options.json = true;
    else if (arg === "--debug") options.debug = true;
    else queryParts.push(arg);
  }
  return { query: queryParts.join(" "), options };
}

function parseAddArgs(args) {
  const options = {};
  let target;
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === "--provider") options.provider = args[++i];
    else if (arg === "--model") options.model = args[++i];
    else if (arg === "--base-url") options.baseUrl = args[++i];
    else if (arg === "--dimensions") options.dimensions = Number(args[++i]);
    else if (arg === "--force") options.force = true;
    else if (!target) target = arg;
  }
  return { target, options };
}

async function handleEmbeddings(args) {
  const subcommand = args[0];
  if (!subcommand) {
    console.log(JSON.stringify({
      active: redactConfig(resolveEmbeddingConfig({ allowMissingApiKey: true })),
      settings: listSettings("embedding.")
    }, null, 2));
    return;
  }

  if (subcommand === "set") {
    const values = parseEmbeddingSetArgs(args.slice(1));
    setSettings(values);
    console.log(JSON.stringify({
      saved: listSettings("embedding."),
      active: redactConfig(resolveEmbeddingConfig({ allowMissingApiKey: true }))
    }, null, 2));
    console.log("Reindex documents after changing provider/model so stored vectors match the active embedding config.");
    return;
  }

  if (subcommand === "test") {
    const text = args.slice(1).join(" ") || "hello world";
    const embedding = await embedText(text);
    console.log(JSON.stringify({
      provider: embedding.provider,
      model: embedding.model,
      dimensions: embedding.dimensions,
      preview: embedding.vector.slice(0, 8)
    }, null, 2));
    return;
  }

  throw new Error(`Unknown embeddings command: ${subcommand}`);
}

function parseEmbeddingSetArgs(args) {
  const values = {};
  for (let i = 0; i < args.length; i += 1) {
    const arg = args[i];
    if (arg === "--provider") values["embedding.provider"] = args[++i];
    else if (arg === "--model") values["embedding.default_model"] = args[++i];
    else if (arg === "--base-url") values["embedding.base_url"] = args[++i];
    else if (arg === "--dimensions") values["embedding.dimensions"] = args[++i];
    else if (arg === "--cloud-enabled") values["embedding.cloud_enabled"] = "true";
    else if (arg === "--local-only") values["embedding.cloud_enabled"] = "false";
  }
  if (values["embedding.provider"] && values["embedding.provider"] !== "local") {
    values["embedding.cloud_enabled"] ??= "true";
  }
  if (values["embedding.provider"] && !values["embedding.default_model"]) {
    values["embedding.default_model"] = defaultModelForProvider(values["embedding.provider"]);
  }
  return values;
}

function redactConfig(config) {
  return {
    provider: config.provider,
    model: config.model,
    dimensions: config.dimensions,
    baseUrl: config.baseUrl,
    apiKey: config.apiKey ? "set" : "missing"
  };
}

async function watchFolder(folder) {
  const resolved = path.resolve(folder);
  console.log(`Watching ${resolved}`);
  await addPath(resolved);
  fs.watch(resolved, { recursive: true }, async (_event, filename) => {
    if (!filename) return;
    const full = path.join(resolved, filename);
    if (fs.existsSync(full) && fs.statSync(full).isFile()) {
      try {
        const [result] = await addPath(full);
        console.log(JSON.stringify(result));
      } catch (error) {
        console.error(error.message);
      }
    }
  });
}
