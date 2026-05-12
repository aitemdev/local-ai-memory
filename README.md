# Local AI Memory

Local-first personal memory for AI tools. It ingests documents, creates canonical Markdown/JSON, chunks and indexes content locally, exposes hybrid search through a CLI, and serves an MCP server so compatible AI clients can query your memory with citations.

This repository is a mostly dependency-free MVP implementation. It uses Node 24 native SQLite, local deterministic embeddings, and a small web UI precursor to the future Tauri app. Document conversion uses Docling first, MarkItDown second, then lightweight Python fallbacks when available.

## Quick start

```bash
node ./bin/mem.js init
node ./bin/mem.js add ./notes
node ./bin/mem.js search "pricing enterprise"
node ./bin/mem.js search "pricing enterprise" --budget normal --limit 8 --debug
node ./bin/mem.js parsers
node ./bin/mem.js embeddings
node ./bin/mem.js serve --mcp
node ./bin/mem.js serve --http --port 3737
```

By default the local store is created in `.memoria/` under the current working directory. Set `MEM_HOME` to use another location.

## Privacy defaults

- Documents, extracted text, chunks and embeddings stay local.
- Cloud embeddings are not used unless configured explicitly.
- The SaaS control plane is represented by local settings placeholders only in this MVP.

## Current format support

- Native support: `.md`, `.txt`, `.csv`, `.tsv`, `.json`, `.html`
- Docling/MarkItDown support when installed: `.pdf`, `.docx`, `.pptx`, `.xlsx`, images and more
- Lightweight fallback support when Python packages are present: PDF via `pypdf`, DOCX via `python-docx`, XLSX via `openpyxl`, PPTX via `python-pptx`

## Parser setup

Check available parser engines:

```bash
node ./bin/mem.js parsers
```

Install the preferred parser stack into a Python environment and point the app at it:

```bash
python -m pip install docling "markitdown[all]"
$env:MEM_PYTHON="C:\Path\To\python.exe"
node ./bin/mem.js parsers
```

If `MEM_PYTHON` is not set, the app tries the bundled Codex Python runtime and then system `python`/`py`.

## Search quality

Search is hybrid by default:

- SQLite FTS retrieves exact lexical matches.
- Local embeddings retrieve semantic matches.
- A local reranker combines semantic score, lexical score, query-term overlap, exact phrase match, term density, compactness and heading signals.
- Budgets cap returned context: `low`, `normal`, `wide`.

Useful CLI options:

```bash
node ./bin/mem.js search "renewal notices" --budget low
node ./bin/mem.js search "renewal notices" --limit 5 --json
node ./bin/mem.js search "renewal notices" --debug
```

## Embedding providers

The default provider is local and sends nothing to the network:

```bash
node ./bin/mem.js embeddings
node ./bin/mem.js embeddings test "hello"
```

OpenAI:

```bash
$env:OPENAI_API_KEY="sk-..."
node ./bin/mem.js embeddings set --provider openai --model text-embedding-3-small --dimensions 1536
node ./bin/mem.js reindex "C:\Path\To\Docs"
```

OpenRouter:

```bash
$env:OPENROUTER_API_KEY="sk-or-..."
node ./bin/mem.js embeddings set --provider openrouter --model openai/text-embedding-3-small --dimensions 1536
node ./bin/mem.js reindex "C:\Path\To\Docs"
```

Ollama local or Ollama Cloud models exposed through Ollama's OpenAI-compatible API:

```bash
ollama pull nomic-embed-text
node ./bin/mem.js embeddings set --provider ollama --model nomic-embed-text --base-url http://localhost:11434/v1
node ./bin/mem.js reindex "C:\Path\To\Docs"
```

API keys are read from environment variables, not stored in SQLite. Changing provider or model requires `reindex` so stored vectors match query vectors.
