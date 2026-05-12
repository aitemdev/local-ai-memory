# Local AI Memory

Local-first personal memory for AI tools. It ingests documents, creates canonical Markdown/JSON, chunks and indexes content locally, exposes hybrid search through a CLI, and serves an MCP server so compatible AI clients can query your memory with citations.

The primary implementation is a Rust binary (`mem`). It uses SQLite (bundled), local deterministic embeddings, and serves a small web UI precursor to the future Tauri app. Document conversion uses Docling first, MarkItDown second, then lightweight Python fallbacks when available.

## Quick start

```bash
cargo run -- init
cargo run -- add ./notes
cargo run -- search "pricing enterprise"
cargo run -- search "pricing enterprise" --budget normal --limit 8 --debug
cargo run -- parsers
cargo run -- embeddings
cargo run -- serve --mcp
cargo run -- serve --http --port 3737
```

Build the release binary once and use it directly:

```bash
cargo build --release
./target/release/mem init
./target/release/mem add ./notes
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
cargo run -- parsers
```

Install the preferred parser stack into a Python environment and point the app at it:

```bash
python -m pip install docling "markitdown[all]"
export MEM_PYTHON=/path/to/python
cargo run -- parsers
```

If `MEM_PYTHON` is not set, the app tries `python3`, then `python`, then `py`.

## Search quality

Search is hybrid by default:

- SQLite FTS retrieves exact lexical matches.
- Local embeddings retrieve semantic matches.
- A local reranker combines semantic score, lexical score, query-term overlap, exact phrase match, term density, compactness and heading signals.
- Budgets cap returned context: `low`, `normal`, `wide`.

Useful CLI options:

```bash
cargo run -- search "renewal notices" --budget low
cargo run -- search "renewal notices" --limit 5 --json
cargo run -- search "renewal notices" --debug
```

## Embedding providers

The default provider is local and sends nothing to the network:

```bash
cargo run -- embeddings
cargo run -- embeddings test "hello"
```

OpenAI:

```bash
export OPENAI_API_KEY=sk-...
cargo run -- embeddings set --provider openai --model text-embedding-3-small --dimensions 1536
cargo run -- reindex /path/to/docs
```

OpenRouter:

```bash
export OPENROUTER_API_KEY=sk-or-...
cargo run -- embeddings set --provider openrouter --model openai/text-embedding-3-small --dimensions 1536
cargo run -- reindex /path/to/docs
```

Ollama local or Ollama Cloud models exposed through Ollama's OpenAI-compatible API:

```bash
ollama pull nomic-embed-text
cargo run -- embeddings set --provider ollama --model nomic-embed-text --base-url http://localhost:11434/v1
cargo run -- reindex /path/to/docs
```

API keys are read from environment variables, not stored in SQLite. Changing provider or model requires `reindex` so stored vectors match query vectors.

## Tests

```bash
cargo test
```

## Legacy Node MVP

The original Node 24 reference implementation lives under `legacy-node/`. It is kept for behavioral comparison and is not the primary path. To run it:

```bash
cd legacy-node
node ./bin/mem.js init
node --test
```
