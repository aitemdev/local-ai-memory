# Conectores

Cómo enchufar Local AI Memory a cada cliente MCP / agente. Todos comparten el mismo modelo: el cliente arranca `mem serve --mcp` como subproceso por stdio JSON-RPC.

---

## Resumen

```
[Cliente IA]  ←stdin/stdout JSON-RPC→  [mem serve --mcp]
                                              ↓
                                  (si daemon vivo: proxy HTTP)
                                              ↓
                                       [mem daemon en 7456]
                                              ↓
                                          [.memoria/]
```

- Sin daemon: `mem serve --mcp` abre `.memoria/` directo (lectura). OK para clientes únicos.
- Con daemon: `mem serve --mcp` detecta `daemon.pid` y proxya `search/get_document/get_chunk/list_collections` por HTTP. Single writer, sin conflictos.

**Recomendado**: arrancar el daemon al login (systemd/launchd, ver MANUAL.md) y dejar que cada cliente lance `mem serve --mcp` como subproceso. El proxy automático garantiza que todos vean el mismo estado.

---

## Tools que expone

| Tool | Args | Devuelve |
| --- | --- | --- |
| `search_memory` | `query` (str), `budget` (low\|normal\|wide, opt), `limit` (num, opt) | Array de chunks con citation, score, breakdown, path |
| `get_document` | `document_id` (str) | Metadata del documento |
| `get_chunk` | `chunk_id` (str) | Texto exacto + source metadata |
| `list_collections` | – | Colecciones (vacío en MVP) |

---

## Variables que afectan a todos los clientes

| Var | Para qué |
| --- | --- |
| `MEM_HOME` | Path absoluto a `.memoria/`. **Recomendado**: setea esto en cada cliente para que todos compartan la misma memoria, sin depender de cwd. |
| `OPENAI_API_KEY` | Solo si embeddings.provider = openai |
| `OPENROUTER_API_KEY` | Solo si embeddings.provider = openrouter |
| `MEM_PYTHON` | Path a python para parsers Docling/MarkItDown |

Ruta canónica sugerida: `~/Library/Application Support/local-ai-memory` (macOS) o `~/.local/share/local-ai-memory` (Linux).

---

## Claude Desktop

Archivo de config:
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`
- Linux: `~/.config/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "local-memory": {
      "command": "/usr/local/bin/mem",
      "args": ["serve", "--mcp"],
      "env": {
        "MEM_HOME": "/Users/TU_USER/Library/Application Support/local-ai-memory"
      }
    }
  }
}
```

Reinicia Claude Desktop. En el chat verás un icono de plugin. Prueba: "What did the team decide about pricing?" y debería invocar `search_memory`.

---

## Claude Code (CLI)

Claude Code tiene MCP via `claude mcp add`:

```bash
claude mcp add local-memory --transport stdio \
  --env MEM_HOME=/Users/TU_USER/.local/share/local-ai-memory \
  -- /usr/local/bin/mem serve --mcp
```

Verifica:

```bash
claude mcp list
```

En sesión: las tools quedan disponibles como `mcp__local-memory__search_memory`, etc.

Alternativa por archivo (`~/.claude/config.json` o `~/.config/claude/config.json`):

```json
{
  "mcpServers": {
    "local-memory": {
      "type": "stdio",
      "command": "/usr/local/bin/mem",
      "args": ["serve", "--mcp"],
      "env": { "MEM_HOME": "/Users/TU_USER/.local/share/local-ai-memory" }
    }
  }
}
```

---

## Cursor

`~/.cursor/mcp.json` (o `Cursor > Settings > MCP`):

```json
{
  "mcpServers": {
    "local-memory": {
      "command": "/usr/local/bin/mem",
      "args": ["serve", "--mcp"],
      "env": {
        "MEM_HOME": "/Users/TU_USER/Library/Application Support/local-ai-memory"
      }
    }
  }
}
```

Cursor lee al arrancar. Comprueba en la paleta "MCP: Tools".

---

## Codex CLI

Codex CLI soporta MCP servers vía `~/.codex/config.toml`:

```toml
[mcp_servers.local-memory]
command = "/usr/local/bin/mem"
args = ["serve", "--mcp"]

[mcp_servers.local-memory.env]
MEM_HOME = "/Users/TU_USER/.local/share/local-ai-memory"
```

Reinicia el CLI. Las tools aparecen disponibles para Codex.

---

## pi_agent_rust

Pi agent reads MCP servers from `~/.pi/mcp.json` (o equivalente en su config dir):

```json
{
  "servers": {
    "local-memory": {
      "transport": "stdio",
      "command": "/usr/local/bin/mem",
      "args": ["serve", "--mcp"],
      "env": {
        "MEM_HOME": "/home/TU_USER/.local/share/local-ai-memory"
      }
    }
  }
}
```

(Ajusta path del binario y MEM_HOME a tu sistema. Verifica el schema exacto con `pi mcp --help`.)

---

## OpenClaw

OpenClaw Gateway carga servers MCP a través de su `config.json` (ubicación típica `~/.openclaw/config.json` o `~/.config/openclaw/`):

```json
{
  "mcp_servers": [
    {
      "id": "local-memory",
      "name": "Local AI Memory",
      "command": "/usr/local/bin/mem",
      "args": ["serve", "--mcp"],
      "env": {
        "MEM_HOME": "/home/TU_USER/.local/share/local-ai-memory"
      }
    }
  ]
}
```

Restart del gateway. Las tools aparecen en el panel.

---

## Cliente MCP genérico

Cualquier cliente que soporte MCP stdio:

- **Comando**: `mem` (asegúrate que esté en el PATH del proceso)
- **Args**: `["serve", "--mcp"]`
- **Transport**: `stdio` (newline-delimited JSON-RPC)
- **Env opcional**: `MEM_HOME=/abs/path/to/.memoria/parent`

Protocolo:

```jsonc
// → initialize
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
// ← capabilities
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"local-ai-memory","version":"0.1.0"}}}

// → tools/list
{"jsonrpc":"2.0","id":2,"method":"tools/list"}

// → tools/call
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_memory","arguments":{"query":"renewal notices","budget":"low"}}}
```

---

## Múltiples clientes en paralelo

Setup recomendado:

1. **Daemon arrancado al login** (systemd/launchd, ver MANUAL.md). Es el único writer.
2. **Cada cliente** arranca su propio `mem serve --mcp`. Estos procesos son lectores; al detectar `daemon.pid` proxyan automáticamente al HTTP.
3. **Watchers** corren en el daemon, no en los procesos MCP. Los clientes ven la memoria actualizada en cuanto el daemon ingiere.

Resultado: Claude Desktop, Cursor y Codex pueden estar abiertos a la vez consultando la misma `.memoria/`, mientras tú dropear archivos en una carpeta watched los hace aparecer en todas las búsquedas en segundos.

---

## Smoke test

Verifica que `mem serve --mcp` responde:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_memory","arguments":{"query":"test"}}}' \
  | mem serve --mcp
```

Tres respuestas JSON por línea. Si no, revisa que `mem init` se ha corrido en `MEM_HOME` y que hay algo indexado.

---

## Troubleshooting de conectores

### El cliente dice "MCP server not responding"

- ¿`mem serve --mcp` en una terminal devuelve respuestas al pipe de arriba? Si no, problema en el binario.
- ¿`MEM_HOME` apunta a un directorio que existe y tiene `.memoria/` dentro? Sin él, `mem` busca `.memoria/` en cwd del cliente.

### El cliente arranca el server pero search_memory devuelve `[]`

- ¿Has añadido docs? `mem status --json` con el mismo MEM_HOME para verificar.
- ¿Provider de embeddings nuevo sin reindex? `mem reindex <carpeta>` necesario tras cambiar.

### Daemon corre pero MCP no proxya

- `mem daemon status` para confirmar.
- El proxy depende de `.memoria/daemon.pid`. El proceso MCP busca ese archivo en `MEM_HOME` (o cwd). Confirma que ambos comparten ruta.

### Cambios indexados en daemon no aparecen en search MCP

- Si el MCP no está proxyando (mismatch MEM_HOME), está leyendo desde un `.memoria/` distinto. Setea el mismo `MEM_HOME` en ambos.

---

## Notas de seguridad

- API keys (OpenAI/OpenRouter) viven en env vars, nunca en SQLite. Si las pasas vía config del cliente, considera que ese archivo no se sincronice a la nube.
- El daemon HTTP solo bindea a `127.0.0.1`. No es accesible desde la red. Si lo quieres remoto, usa SSH tunneling, no abras el puerto.
- Las búsquedas y ingests no llaman a la red salvo que provider sea `openai`/`openrouter`. Con `local` u `ollama` todo es local.
