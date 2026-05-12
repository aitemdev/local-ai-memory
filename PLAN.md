# Local AI Memory: Continuation Plan

## Current State

The repository contains two implementations:

- **Node MVP**: currently working and tested. It is the reference implementation for behavior.
- **Rust migration**: scaffolded but not yet compiled on Windows because MSVC `link.exe` was unavailable. Continue this on Linux.

GitHub repo:

- `https://github.com/aitemdev/local-ai-memory`
- Main branch: `main`

## What Works Today

Node CLI:

- `init`
- `add`
- `search`
- `ask`
- `status`
- `parsers`
- `embeddings`
- `embeddings set`
- `embeddings test`
- `reindex`
- `serve --mcp`
- `serve --http`

Document parsing:

- Native text: `.md`, `.txt`, `.csv`, `.tsv`, `.json`, `.html`
- Docling and MarkItDown integration through `tools/parse_document.py`
- Python fallbacks: PDF via `pypdf`, DOCX via `python-docx`, XLSX via `openpyxl`, PPTX via `python-pptx`

Search:

- SQLite FTS for lexical search
- Local deterministic embeddings for semantic search
- Local reranker combining lexical, semantic, term overlap, phrase match, density, compactness and heading signals
- Token budgets: `low`, `normal`, `wide`
- Debug output: `--debug`
- JSON output: `--json`

Embeddings:

- `local`
- `openai`
- `openrouter`
- `ollama` / Ollama Cloud via OpenAI-compatible `/v1/embeddings`
- API keys are read from environment variables, not stored in SQLite.

Tests:

```bash
node --test
```

Expected: all Node tests pass.

## Immediate Next Step on Linux

Clone and compile Rust:

```bash
git clone https://github.com/aitemdev/local-ai-memory.git
cd local-ai-memory
cargo check
```

First goal: make the Rust CLI compile and match the Node CLI for:

```bash
cargo run -- init
cargo run -- add ./notes
cargo run -- search "pricing enterprise" --debug
cargo run -- embeddings
cargo run -- parsers
```

The Rust source is under `rust/`. The Node source under `src/` is the behavioral reference.

## Rust Migration Tasks

1. **Fix Rust compile errors**
   - Run `cargo check`.
   - Fix type/signature issues in `rust/indexer.rs`, `rust/cli.rs`, and module boundaries.
   - Keep the existing SQLite schema compatible with the Node MVP.

2. **Add Rust tests**
   - Port the Node tests from `test/` into Rust integration tests.
   - Required scenarios:
     - Ingest Markdown and search it.
     - Exact lexical match ranks above nearby semantic match.
     - Embedding provider config resolves correctly.
     - Parser probe does not throw.

3. **Complete Rust CLI parity**
   - Match Node commands and flags.
   - Keep scriptable/non-interactive CLI as the base interface.
   - Add clear errors for missing API keys and parser dependencies.

4. **Add MCP server in Rust**
   - Port `src/mcp.js`.
   - Required tools:
     - `search_memory`
     - `get_document`
     - `get_chunk`
     - `list_collections`
   - Use newline-delimited JSON-RPC over stdin/stdout.

5. **Add HTTP/UI server in Rust**
   - Port `src/http.js` or replace with a small Rust HTTP server.
   - Serve `public/`.
   - Endpoints:
     - `GET /api/status`
     - `GET /api/search?q=...&budget=...`
     - `POST /api/add`

6. **Retire or demote Node**
   - Once Rust reaches feature parity, update README to use `cargo run --` or `mem`.
   - Keep Node as `legacy-node/` or remove it after confidence is high.

## Product Tasks After Rust Parity

1. **Interactive TUI**
   - Prefer Rust `ratatui` + `crossterm` instead of Go Bubble Tea to avoid a third runtime.
   - Use it for human configuration:
     - provider selection
     - model selection
     - parser diagnostics
     - reindex status
     - search preview

2. **Better Vector Store**
   - Current vectors are JSON in SQLite.
   - Next options:
     - Keep SQLite for MVP simplicity.
     - Add LanceDB when the Rust path is stable.
     - Consider Qdrant only if a server-style local daemon becomes necessary.

3. **Real Local Embeddings**
   - Current local embedding is deterministic hash-based, useful for fallback/testing.
   - Add one of:
     - Ollama `nomic-embed-text`
     - BGE-M3 through a local model runtime
     - FastEmbed if Rust ecosystem support is good enough

4. **Daemon and File Watching**
   - Add `mem watch <folder>` in Rust with `notify`.
   - Track changed files and reindex only modified content.
   - Keep jobs in SQLite.

5. **Tauri Desktop App**
   - Use Rust core directly.
   - Keep the app simple:
     - drag and drop
     - search
     - document status
     - parser/embedding settings

6. **SaaS Control Plane Later**
   - Account, license, feature flags, settings sync.
   - No document/chunk/embedding sync by default.
   - Preserve local-first privacy.

## Acceptance Criteria for Rust MVP

Rust can replace Node when all are true:

- `cargo test` passes.
- `cargo run -- init` creates `.memoria/`.
- `cargo run -- add <folder>` ingests Markdown and at least DOCX/PDF through Python parser.
- `cargo run -- search "<query>" --debug` returns cited, reranked chunks.
- `cargo run -- embeddings set ...` persists provider config.
- `cargo run -- embeddings test "hello"` works for local and for cloud when API keys are present.
- MCP server works with `tools/list` and `tools/call`.
- README no longer depends on Node for the primary flow.

## Known Caveats

- Rust migration was not compiled on Windows because MSVC Build Tools/linker were missing.
- Docling may be slow on first run because it loads heavy dependencies/models.
- Changing embedding provider/model requires `reindex`.
- API keys must remain environment-only.
- The GitHub repo is private under `aitemdev/local-ai-memory`.
