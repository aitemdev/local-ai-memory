# Product

## Register

product

## Users

Developers and knowledge workers who run AI assistants (Claude Code, Cursor, ChatGPT, MCP-compatible clients) and want those assistants to ground answers in their own documents — meeting notes, RFCs, research, contracts. Mostly macOS, terminal-fluent, allergic to telemetry. They reach for this when an assistant guessed instead of citing, or when they want a search box that lives on their machine and does not phone home.

## Product Purpose

A local-first memory layer that an AI client can query through MCP, HTTP, or the CLI. Documents are ingested, chunked, indexed in SQLite FTS, embedded into LanceDB tables per provider/model, and reranked with a transparent hybrid scoring model. The desktop app is the human face of the same Rust core: drag in a folder, watch it index, search it, switch embedding provider, see exactly which chunk grounded a result. Success is measured in trust — every answer must surface its citation, and the user must always be able to point at the file, page, or heading that produced it.

## Brand Personality

Precise, calm, trustworthy. Technical-warm rather than corporate-cold. The product talks like a senior engineer who finishes their sentences and respects your time. It is not playful, not whimsical, not aspirational. It does not perform "AI". It performs reliability.

## Anti-references

- Generic SaaS dashboards (Vercel-clone, Linear-clone, Supabase-clone) with their tile grids and metric heroes
- ChatGPT-style boxy chat shells dressed up as a "memory app"
- Notion's cream and rounded everything — wrong personality
- AI-startup neon, purple gradients, "magic sparkle" iconography
- Side-stripe accent borders, gradient text, glass cards used decoratively
- Dark-mode-only fetishism: looking technical by being uniformly black

## Strategic Design Principles

1. **Privacy is visible.** An offline pill, no "Cloud" labels by default, no surfaces that imply data leaves the machine. When cloud embeddings are configured, that fact is shown plainly, not boasted about.
2. **Citations are first-class.** Title, file, page, heading, score breakdown are part of the result body, not a tooltip or a debug panel. The user should never have to click to find out where text came from.
3. **Search feels instant.** 220ms debounced query, no spinners under 400ms, results animate in only on the first render of a new query.
4. **Density tuned for power users.** Denser than ChatGPT, looser than Linear. Single-screen library overview without scrolling on a 13" laptop.
5. **Settings disclose tradeoffs.** Each embedding provider explains what it costs (network, accuracy, latency). No silent defaults that the user discovers months later.
