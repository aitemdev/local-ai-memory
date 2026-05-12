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

## Storage layout

`.memoria/` contains:

- `memory.sqlite` — documents, chunks, FTS, jobs, settings.
- `lance/<provider>_<model>.lance/` — vectors per embedding model, stored in a LanceDB dataset. Each provider/model lives in its own table so multiple embedding configs can coexist.
- `canonical/` and `originals/` — extracted Markdown/JSON and source files.

Switching embedding provider or model requires `reindex` so the active Lance table contains vectors matching your queries.

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

## Persistent daemon

```bash
cargo run -- daemon --port 7456
```

The daemon is the long-running service that holds the index alive across desktop launches, agent sessions, and CLI calls. On start it:

- Resumes watchers from the `watch.folders` setting (initial scan + live debounce).
- Serves the HTTP API on `localhost:7456`.
- Logs every ingest event as JSON on stderr.

HTTP endpoints:

| Method | Path | Body / query | Purpose |
| --- | --- | --- | --- |
| GET | `/api/status` | – | Document and job counts |
| GET | `/api/search` | `?q=...&budget=low|normal|wide&limit=N` | Hybrid search |
| POST | `/api/add` | `{"paths":[...], "force":false}` | Ingest a one-shot batch |
| POST | `/api/reset` | – | Wipe all documents, chunks, vectors |
| GET | `/api/parsers` | – | Parser probe |
| GET | `/api/embeddings` | – | Active embedding config |
| POST | `/api/embeddings/set` | `{"provider":"ollama","model":"nomic-embed-text",...}` | Switch provider |
| GET | `/api/watched` | – | List watched folders |
| POST | `/api/watch` | `{"path":"/abs/path"}` | Start watching |
| POST | `/api/unwatch` | `{"path":"/abs/path"}` | Stop watching |

The static UI at `public/` is also served from the daemon, so a browser at `http://localhost:7456` becomes a lightweight client.

## Desktop app

A Tauri 2 desktop shell sits in `src-tauri/` with the frontend in `dist/`. It reuses the same Rust core (search, ingest, embeddings, parsers) through `#[tauri::command]` handlers.

```bash
cargo build --manifest-path src-tauri/Cargo.toml
./target/debug/local-ai-memory   # dev run
```

The UI follows a macOS aesthetic: 220 px sidebar, traffic-light inset overlay titlebar, three sections (Search · Library · Settings), drag-and-drop ingest, instant search with citations, embedding-provider switcher and parser diagnostics. Light/dark mode follows the system. No npm; everything ships from `dist/` as static assets.

For a signed `.dmg` on macOS:

```bash
cargo install tauri-cli --version "^2.0"
cargo tauri build
```

## Interactive TUI

```bash
cargo run -- tui
```

Keyboard shortcuts:

- `Tab` / `Shift+Tab`: switch panel
- Config panel: `↑/↓` to pick a provider, `Enter` to persist (reindex required after)
- Search panel: type a query, `Enter` to search, `Esc` to clear, `Ctrl+C` to quit
- `q`: quit from any non-input panel

## File watching

Reindex on the fly while you edit:

```bash
cargo run -- watch ./notes
```

The watcher runs an initial scan, then debounces filesystem events (~750 ms) before reindexing modified or newly created files in place.

## Tests

```bash
cargo test
```

The Ollama smoke test is gated behind an environment variable so the default suite stays offline:

```bash
MEM_OLLAMA_TEST=1 cargo test -- --ignored
```

## Legacy Node MVP

The original Node 24 reference implementation lives under `legacy-node/`. It is kept for behavioral comparison and is not the primary path. To run it:

```bash
cd legacy-node
node ./bin/mem.js init
node --test
```
