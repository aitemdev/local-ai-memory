# Manual de Nolost

Guía paso a paso para usar la herramienta sin haber leído el código.

---

## Qué es esto

Nolost es una **memoria privada** para asistentes de IA. Coges una carpeta con tus documentos (notas, PDFs, contratos, RFCs, lo que sea), la indexa en tu máquina, y luego puedes:

- Buscar texto con citas (qué archivo, qué chunk, qué heading)
- Conectarla a Claude Code / Cursor / ChatGPT vía MCP para que esos agentes consulten tu memoria con citas reales
- Tener una app de escritorio o un servidor HTTP local para usarla

**Nada sale de tu máquina por defecto.** Embeddings, búsqueda y almacenamiento son locales.

---

## Vocabulario mínimo

Sin esto el resto no se entiende.

| Palabra | Qué significa |
| --- | --- |
| **Documento** | Un archivo entero (un .md, un .pdf, etc.) |
| **Chunk** | Trozo del documento (~600 tokens). La búsqueda devuelve chunks, no docs enteros. |
| **Embedding** | Vector numérico que representa el significado del chunk. Permite búsqueda semántica. |
| **FTS** | Full-text search de SQLite. Búsqueda léxica (palabras exactas). |
| **Reranker** | Algoritmo local que mezcla varias señales (semántico + léxico + overlap + heading + densidad) en una sola puntuación. |
| **Budget** | Cuánto contexto devuelve la búsqueda: `low` poco, `normal` medio, `wide` mucho. |
| **MCP** | Model Context Protocol. Permite que Claude/Cursor llamen tools de tu memoria. |
| **Daemon** | Proceso en background siempre vivo que mantiene el índice listo. |
| **.memoria/** | Carpeta donde se guarda todo (SQLite + vectores Lance + extractos). |

---

## Instalación

Requisitos: Rust >= 1.84 con `cargo`. Opcional: Python para PDFs/DOCX/XLSX/PPTX.

```bash
git clone https://github.com/aitemdev/nolost.git
cd nolost
cargo build --release
```

El binario queda en `./target/release/mem`. Para tenerlo en el PATH:

```bash
cp ./target/release/mem ~/.local/bin/    # Linux
cp ./target/release/mem /usr/local/bin/   # macOS (sudo si hace falta)
```

Verifica:

```bash
mem --help
```

---

## El "Hola, mundo" en 60 segundos

```bash
cd /carpeta/donde/tengas/tus/notas
mem init                                  # crea .memoria/ aqui
mem add ./                                # indexa todo
mem search "pricing renewal"              # busca y muestra resultados
```

Cada `mem <comando>` es independiente. No hace falta nada corriendo en background (todavía).

---

## Comandos del CLI uno por uno

### `mem init`

Crea la carpeta `.memoria/` en el directorio actual con SQLite, schema y subcarpetas vacías.

```bash
mem init
# Initialized local memory store at /tu/path/.memoria
```

Si ya existe, no rompe nada, solo asegura el schema.

**Tip:** `MEM_HOME=/otra/ruta mem init` la pone en otra ruta. Útil si quieres una memoria global compartida entre proyectos.

---

### `mem add <ruta>`

Ingiere un archivo o una carpeta completa (recursivo).

```bash
mem add ./notas                           # carpeta
mem add ./contratos/2025-acme.pdf         # un solo archivo
```

Salida JSON por línea:

```json
{"file":"/abs/path/foo.md","document_id":"abcd...","status":"ready","chunks":3,"error":null}
```

- `status: ready` → indexado correctamente
- `status: unchanged` → mismo hash que la última vez, no se reindexó
- `status: error` → el campo `error` te dice por qué

**Flags útiles:**

```bash
mem add ./docs --force                    # reindexa aunque no haya cambiado
mem add ./docs --provider ollama          # usa ese provider solo para esta operación
```

---

### `mem search <query>`

Búsqueda híbrida (semántica + léxica + reranker).

```bash
mem search "enterprise pricing strategy"
```

Salida:

```
1. pricing.md (pricing.md) score=0.91 tokens=35 chunk=8b1d...
   # Pricing Notes Enterprise tier costs $500/month per seat. ...
2. roadmap.md (roadmap.md) score=0.03 tokens=20 chunk=9d01...
   # Roadmap Q3 - Ship vector search beta - Migrate to Rust core ...
```

**Flags:**

```bash
mem search "rollout" --budget normal      # mas contexto (low|normal|wide)
mem search "rollout" --limit 5            # max 5 resultados
mem search "rollout" --json               # JSON estructurado
mem search "rollout" --debug              # añade score_breakdown + path completo
```

Con `--debug` ves cómo el reranker compuso el score:

```
scores={"compactness":1.0,"lexical":0.0,"overlap":0.0,"phrase":0.0,"semantic":1.0}
```

Los pesos del reranker son: `semantic 0.32, lexical 0.24, overlap 0.18, phrase 0.14, compactness 0.03`.

---

### `mem ask <query>`

Igual que `search` pero con budget por defecto `normal` (más contexto). Pensado para preguntas del estilo "qué decidimos sobre X".

```bash
mem ask "como gestionamos los refunds"
```

---

### `mem status`

Cuántos documentos, en qué estado, cuántos jobs en cola.

```bash
mem status
```

```json
{
  "documents": [{ "status": "ready", "count": 42 }],
  "jobs": []
}
```

---

### `mem reset --yes`

Borra TODO. Documents, chunks, FTS, vectores Lance, extractos canónicos. Originales NO se tocan (nunca se copiaron).

```bash
mem reset --yes
```

Sin `--yes` falla intencionalmente para evitar accidentes.

---

### `mem reindex <ruta>`

Como `add --force`. Útil cuando cambias provider o modelo de embeddings (los vectores viejos no sirven con el nuevo modelo).

```bash
mem reindex ./notas
```

---

### `mem parsers`

Comprueba qué parsers Python están disponibles para PDF/DOCX/XLSX/PPTX/imágenes.

```bash
mem parsers
```

Si dice `ready: false` con `python: null`, necesitas Python en el PATH o `MEM_PYTHON=/ruta/python`.

Para parsers reales:

```bash
pip install docling "markitdown[all]" pypdf python-docx openpyxl python-pptx
mem parsers
```

Docling es el mejor pero lento la primera vez (descarga modelos).

---

### `mem embeddings`

Muestra el provider/modelo activo + settings persistidos.

```bash
mem embeddings
```

#### `mem embeddings set --provider <p>`

Cambia el provider. Opciones: `local` (default, hash-based, 128 dims, offline), `ollama`, `openai`, `openrouter`.

```bash
# Ollama local (recomendado para semántica real, requiere Ollama corriendo)
mem embeddings set --provider ollama --model nomic-embed-text --base-url http://localhost:11434/v1

# OpenAI (cloud, requiere OPENAI_API_KEY env var)
mem embeddings set --provider openai --model text-embedding-3-small --dimensions 1536

# OpenRouter (cloud, requiere OPENROUTER_API_KEY)
mem embeddings set --provider openrouter --model openai/text-embedding-3-small --dimensions 1536

# Vuelta al local
mem embeddings set --provider local
```

**IMPORTANTE:** al cambiar provider o modelo necesitas `mem reindex <carpeta>` porque los vectores viejos están en otra "dimensión" semántica.

#### `mem embeddings test "texto"`

Genera un embedding del texto y te muestra dimensiones + preview de los primeros 8 floats. Útil para verificar que el provider funciona.

```bash
mem embeddings test "hola mundo"
```

---

### `mem watch <carpeta>`

Mantiene la carpeta sincronizada. Hace un scan inicial y luego se queda vivo. Cuando creas/modificas un archivo dentro, lo reindexa con debounce de 750ms.

```bash
mem watch ./notas
# Ctrl-C para parar
```

Sale del proceso = deja de vigilar. Para que sea permanente usa el **daemon** (siguiente sección).

---

### `mem serve --mcp`

Inicia un servidor MCP por stdin/stdout. Los clientes IA (Claude Desktop, Cursor, etc.) hablan con él vía JSON-RPC.

```bash
mem serve --mcp
```

Expone 4 tools:

- `search_memory(query, budget?, limit?)`
- `get_document(document_id)`
- `get_chunk(chunk_id)`
- `list_collections()`

Configuración en Claude Desktop (`~/Library/Application Support/Claude/claude_desktop_config.json` en macOS):

```json
{
  "mcpServers": {
    "local-memory": {
      "command": "/usr/local/bin/mem",
      "args": ["serve", "--mcp"]
    }
  }
}
```

Reinicia Claude Desktop. Ahora cuando le preguntes algo, puede llamar `search_memory` para citar tus docs.

---

### `mem serve --http --port 7456`

Servidor HTTP local con la misma funcionalidad. Endpoints en sección **Daemon** más abajo. Sirve también la web UI estática.

```bash
mem serve --http --port 7456
# abre http://localhost:7456 en el browser
```

---

### `mem tui`

Interface texto interactivo (ratatui). Pestañas: Config, Parsers, Status, Search. Útil cuando estás en un servidor sin GUI.

Atajos:
- `Tab` / `Shift+Tab`: cambiar pestaña
- En Config: `↑/↓` para elegir provider, `Enter` para aplicar
- En Search: escribe + `Enter`; `Esc` limpia; `Ctrl+C` sale
- `q`: salir desde pestañas que no son input

---

## El Daemon (proceso siempre vivo)

Lo de arriba funciona pero cada comando abre/cierra DB. Si quieres una memoria que viva 24/7, reindexe automático, y que CLI/desktop/agentes IA compartan estado, usa el daemon.

### Arrancar

```bash
mem daemon                                # foreground, puerto 7456
mem daemon --port 7500                    # otro puerto
```

Mientras está corriendo:
- Resume watchers persistidos automáticamente (carpetas que añadiste vía desktop o `/api/watch`)
- Sirve HTTP en localhost:7456
- Loguea cada ingest en stderr como JSON

### Controlar

```bash
mem daemon status                         # {running, pid, port}
mem daemon stop                           # graceful SIGTERM, fallback SIGKILL 3s
mem daemon restart
```

### Autodeteccion del CLI

Si daemon está vivo, `mem status`, `mem search`, `mem add`, `mem parsers`, `mem reset` automáticamente llaman vía HTTP en vez de abrir DB directo. Esto evita conflictos de escritura concurrentes. Si daemon no está vivo, fallback transparente a modo directo.

Tú no haces nada distinto; siempre escribes `mem search "x"`. El binario decide.

### Endpoints HTTP

```
GET  /api/status                          → {documents, jobs}
GET  /api/search?q=...&budget=low&limit=N → [{title, citation, score, ...}]
POST /api/add {paths:[...], force:false}  → [ingest results]
POST /api/reset                           → {documents, chunks} (cantidad borrada)
GET  /api/documents                       → [{id, title, path, chunks, ...}]
POST /api/document/delete {id}            → {id, path}
GET  /api/parsers                         → status parsers
GET  /api/embeddings                      → {active, settings}
POST /api/embeddings/set {provider,...}   → {ok:true}
GET  /api/watched                         → ["/abs/path", ...]
POST /api/watch   {path}                  → lista actualizada
POST /api/unwatch {path}                  → lista actualizada
```

Ejemplo curl:

```bash
curl -X POST -d '{"path":"/home/abel/notas"}' http://localhost:7456/api/watch
curl "http://localhost:7456/api/search?q=pricing&budget=low" | jq
```

### Autostart al login

**Linux (systemd user):**

```bash
install -Dm644 bin/nolost.service ~/.config/systemd/user/nolost.service
systemctl --user daemon-reload
systemctl --user enable --now nolost.service
journalctl --user -u nolost -f       # ver logs
```

**macOS (launchd):**

```bash
install -Dm644 bin/dev.aitemdev.nolost.plist ~/Library/LaunchAgents/dev.aitemdev.nolost.plist
launchctl load ~/Library/LaunchAgents/dev.aitemdev.nolost.plist
```

Edita el archivo si tu `mem` no está en `/usr/local/bin/mem` o `~/.local/bin/mem`.

---

## Cuándo usar qué

| Caso | Herramienta |
| --- | --- |
| Probar rápido, una vez | `mem add` + `mem search` |
| Carpeta que cambia mucho, una sesión de trabajo | `mem watch` |
| Carpeta permanente, agentes IA leyendo siempre | Daemon con autostart |
| Quiero ver mis docs visualmente, drag-drop | App desktop (`./src-tauri/target/debug/nolost`) |
| Necesito búsqueda semántica buena | `mem embeddings set --provider ollama` + reindex |
| Conectar Claude Code | `mem serve --mcp` registrado en claude_desktop_config.json |
| API para automatizar | Daemon + curl al puerto |

---

## App desktop

Build:

```bash
cargo build --manifest-path src-tauri/Cargo.toml --release
./src-tauri/target/release/nolost
```

Tres secciones (atajos ⌘1/⌘2/⌘3):

- **Search**: input grande, segmented Focused/Balanced/Wide, resultados con citation + breakdown
- **Library**: stats arriba, "Watched folders" (autoindex permanente), dropzone, lista de docs con Remove
- **Settings**: provider picker, parser status, store info

Drag-drop una carpeta → añade a watched list automáticamente → ingest progresivo con barra + cancel.

---

## Variables de entorno

| Variable | Para qué |
| --- | --- |
| `MEM_HOME` | Cambia la ubicación de `.memoria/` |
| `MEM_PYTHON` | Path al python para Docling/MarkItDown |
| `MEM_EMBEDDING_PROVIDER` | Override del provider para una sesión |
| `MEM_EMBEDDING_MODEL` | Override del modelo |
| `MEM_EMBEDDING_BASE_URL` | Override del endpoint |
| `MEM_EMBEDDING_DIMENSIONS` | Override de dims |
| `OPENAI_API_KEY` | Key para provider openai |
| `OPENROUTER_API_KEY` | Key para provider openrouter |
| `OLLAMA_API_KEY` | Key opcional para Ollama Cloud (local Ollama no necesita) |

Las API keys NUNCA se guardan en SQLite. Solo viven en env.

---

## Qué hay en `.memoria/`

```
.memoria/
├── memory.sqlite                  # documents, chunks, FTS, settings, jobs
├── memory.sqlite-shm              # SQLite WAL shared memory
├── memory.sqlite-wal              # write-ahead log
├── lance/
│   ├── local_local_hash_v1.lance/ # vectores del provider local
│   └── ollama_nomic_embed_text.lance/  # vectores Ollama
├── canonical/                     # markdown/json extraidos
├── originals/                     # vacio en el MVP (no copiamos)
├── logs/                          # vacio en el MVP
└── daemon.pid                     # {pid, port} cuando daemon corre
```

Borrar `.memoria/` entera = empezar de cero. Tus originales NO se tocan.

---

## Problemas comunes

### "Missing API key for openai"

Set `OPENAI_API_KEY` antes de ejecutar:

```bash
export OPENAI_API_KEY=sk-...
mem embeddings test "hola"
```

### Búsqueda no encuentra nada que sé que está

1. ¿`mem status` lo lista como `ready`?
2. Tras cambiar provider: `mem reindex` la carpeta
3. Si los embeddings son `local` (hash), la búsqueda semántica es flojísima. Cambia a `ollama` (real) y reindexa.

### `mem parsers` dice not ready

```bash
which python3 || which python
# si nada, instala Python
pip install docling pypdf python-docx openpyxl python-pptx
mem parsers
```

### Daemon no arranca: "daemon already running"

```bash
mem daemon status                          # ver si esta vivo de verdad
mem daemon stop                            # mata si si
rm .memoria/daemon.pid                     # ultimo recurso si el archivo quedo huerfano
mem daemon
```

### Desktop crashea al arrastrar carpeta gigante

Era un bug antiguo (>200 docs bloqueaba IPC). Está fixed: ingest corre en thread separado con barra de progreso + botón Cancel. Si todavía crashea con miles de docs, abre issue.

### Wayland error 71 al arrancar desktop

```bash
LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1 ./src-tauri/target/release/nolost
```

O usa el wrapper:

```bash
./bin/run-app.sh
```

---

## Flujo recomendado para usuario "instalo y olvido"

```bash
# Una vez en la vida:
cargo build --release
cp target/release/mem ~/.local/bin/
install -Dm644 bin/nolost.service ~/.config/systemd/user/
systemctl --user enable --now nolost.service

# Ollama para semántica real (opcional pero recomendado):
ollama pull nomic-embed-text
mem embeddings set --provider ollama --model nomic-embed-text --base-url http://localhost:11434/v1

# Vigilar tus carpetas (una vez):
curl -X POST -d '{"path":"/home/yo/notas"}' http://localhost:7456/api/watch
curl -X POST -d '{"path":"/home/yo/work"}'  http://localhost:7456/api/watch

# Conectar Claude Desktop con MCP:
# editar ~/Library/Application Support/Claude/claude_desktop_config.json
# (ver seccion mem serve --mcp arriba)
```

A partir de aquí: dropear archivos en `/home/yo/notas` los indexa automático. `mem search "X"` desde cualquier terminal. Claude consulta vía MCP.

---

## Tests y verificación

```bash
cargo test --lib                           # 6 tests unitarios
MEM_OLLAMA_TEST=1 cargo test -- --ignored  # smoke test Ollama (necesita Ollama corriendo)
```

---

## Comandos en orden alfabético (cheatsheet)

```
mem add <ruta> [--force] [--provider]
mem ask <query> [--budget] [--limit] [--json] [--debug]
mem daemon [start|stop|status|restart] [--port N]
mem embeddings [set|test] [...]
mem init
mem parsers
mem reindex <ruta>
mem reset --yes
mem search <query> [--budget low|normal|wide] [--limit] [--json] [--debug]
mem serve [--mcp | --http --port N]
mem status
mem tui
mem watch <carpeta>
```

---

Si algo de aquí no funciona o falta, abre issue o pregúntale al MCP-yo-mismo que acabas de configurar.
