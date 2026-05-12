import { searchMemory, getDocument, getChunk, listCollections } from "./indexer.js";

const tools = [
  {
    name: "search_memory",
    description: "Search local indexed memory and return grounded chunks with citations.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string" },
        budget: { type: "string", enum: ["low", "normal", "wide"] },
        limit: { type: "number" }
      },
      required: ["query"]
    }
  },
  {
    name: "get_document",
    description: "Return metadata for a local document.",
    inputSchema: {
      type: "object",
      properties: { document_id: { type: "string" } },
      required: ["document_id"]
    }
  },
  {
    name: "get_chunk",
    description: "Return exact chunk text and source metadata.",
    inputSchema: {
      type: "object",
      properties: { chunk_id: { type: "string" } },
      required: ["chunk_id"]
    }
  },
  {
    name: "list_collections",
    description: "List local memory collections.",
    inputSchema: { type: "object", properties: {} }
  }
];

export function serveMcp() {
  process.stdin.setEncoding("utf8");
  let buffer = "";
  process.stdin.on("data", async (chunk) => {
    buffer += chunk;
    const lines = buffer.split("\n");
    buffer = lines.pop() || "";
    for (const line of lines) {
      if (!line.trim()) continue;
      await handleLine(line);
    }
  });
}

async function handleLine(line) {
  let message;
  try {
    message = JSON.parse(line);
    const result = await handleRequest(message);
    if (message.id !== undefined) write({ jsonrpc: "2.0", id: message.id, result });
  } catch (error) {
    write({
      jsonrpc: "2.0",
      id: message?.id ?? null,
      error: { code: -32000, message: error.message }
    });
  }
}

async function handleRequest(message) {
  if (message.method === "initialize") {
    return {
      protocolVersion: "2024-11-05",
      capabilities: { tools: {} },
      serverInfo: { name: "local-ai-memory", version: "0.1.0" }
    };
  }
  if (message.method === "tools/list") return { tools };
  if (message.method === "tools/call") {
    const { name, arguments: args = {} } = message.params || {};
    if (name === "search_memory") {
      const rows = await searchMemory(args.query, { budget: args.budget, limit: args.limit });
      return asContent(rows);
    }
    if (name === "get_document") return asContent(getDocument(args.document_id));
    if (name === "get_chunk") return asContent(getChunk(args.chunk_id));
    if (name === "list_collections") return asContent(listCollections());
    throw new Error(`Unknown tool: ${name}`);
  }
  return {};
}

function asContent(value) {
  return {
    content: [
      {
        type: "text",
        text: JSON.stringify(value, null, 2)
      }
    ]
  };
}

function write(message) {
  process.stdout.write(`${JSON.stringify(message)}\n`);
}
