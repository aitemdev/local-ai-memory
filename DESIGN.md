# Design

## Visual Theme

**Editorial-technical, macOS-native.** The reference is the Tahoe / Big Sur sidebar pattern — a lighter sidebar acts as the source of light, the main content sits in a slightly recessed warmer surface, and the user reads documents in a column not a grid. Light and dark modes are both first-class and visually distinct (light is warm paper, dark is a soft graphite, neither is jet-black). Surfaces are mostly flat; depth comes from tint variation and 1px borders, not shadows. Motion is restrained and exponential.

## Color

OKLCH throughout. Restrained strategy: tinted neutrals + one accent at <10% of surface area.

### Light

| Role | OKLCH | Hex | Use |
| --- | --- | --- | --- |
| Sidebar | `oklch(0.985 0.003 250)` | `#fbfbfd` | Lightest surface — source of light |
| Surface | `oklch(0.965 0.004 250)` | `#f4f4f7` | Main content background |
| Card | `oklch(1.000 0 0)` | `#ffffff` | Result cards, settings sections |
| Border | `oklch(0.91 0.005 250 / 0.7)` | tinted gray | 1px hairlines |
| Text | `oklch(0.22 0.01 270)` | `#1d1d22` | Primary |
| Text muted | `oklch(0.55 0.01 270)` | `#797982` | Secondary |
| Text faint | `oklch(0.72 0.01 270)` | `#a8a8b0` | Tertiary, eyebrows |
| Accent | `oklch(0.62 0.18 250)` | `#0a84ff` | Selection, focus, key actions |
| Success | `oklch(0.72 0.16 152)` | `#34c759` | Status pill dot |
| Danger | `oklch(0.65 0.21 25)` | `#ff453a` | Errors only |

### Dark

| Role | OKLCH | Hex | Use |
| --- | --- | --- | --- |
| Sidebar | `oklch(0.245 0.005 270)` | `#2c2c30` | Lightest surface in dark mode |
| Surface | `oklch(0.195 0.006 270)` | `#22222a` | Main content background |
| Card | `oklch(0.225 0.006 270)` | `#28282f` | Result cards |
| Border | `oklch(0.32 0.008 270 / 0.6)` | tinted | 1px hairlines |
| Text | `oklch(0.96 0.005 270)` | `#f4f4f7` | Primary |
| Text muted | `oklch(0.7 0.01 270)` | `#a8a8b2` | Secondary |
| Text faint | `oklch(0.55 0.01 270)` | `#7a7a85` | Tertiary |
| Accent | `oklch(0.7 0.16 250)` | `#3a96ff` | Slightly brighter for dark surfaces |

No `#000`, no `#fff` for body text. Neutrals tinted ~250° (cool blue-violet) at chroma 0.005, matching the accent hue. Citations and metadata never use the accent — they live in muted neutrals so the accent stays scarce.

## Typography

System stack: `-apple-system, BlinkMacSystemFont, "SF Pro Display", "SF Pro Text", "Helvetica Neue", sans-serif`. Mono stack: `"SF Mono", ui-monospace, Menlo, monospace`.

| Token | Size | Weight | Letter spacing | Line height | Use |
| --- | --- | --- | --- | --- | --- |
| Display | 24px | 600 | -0.4px | 1.15 | Section titles in panels |
| Title | 16px | 600 | -0.2px | 1.25 | Result card title, settings section heads |
| Body | 13.5px | 400 | 0 | 1.55 | Result snippets, prose |
| Body strong | 13.5px | 500 | 0 | 1.5 | UI labels, nav items |
| Eyebrow | 10.5px | 600 | 0.8px (uppercase) | 1.2 | Citations, "RECENT", "MODEL" |
| Caption | 11.5px | 400 | 0 | 1.4 | Score numerals, paths |
| Mono | 11.5px | 450 | 0 | 1.55 | Chunk excerpts, paths, scores |

Tabular numerals (`font-variant-numeric: tabular-nums`) on every score, count, and chunk id so columns align without manual width.

Body measure capped at 70ch in result snippets so reading rhythm holds even on a wide window.

## Layout

Two-column grid. 220px sidebar, 1fr content. Both panes scroll independently.

- **Sidebar**: 14px outer padding. Brand block reserves 60px top-inset for the traffic-light cluster. Nav items 30px tall with 18px left rail for active indicator bullet. Footer pill anchored to bottom.
- **Titlebar**: 38px overlay strip on the content side, drag region, hairline border at the bottom only.
- **Content padding**: 32px horizontal, 24px top, 40px bottom. No outer card wrapper.
- **Rhythm**: spacing scale `[2, 4, 6, 10, 14, 20, 28, 40, 64]` px. Not every gap is the same; vertical groupings use 14/20/28 to create visible sections without dividers.

Cards used only for result rows and settings panels. Never nested. Drop zone is a styled fieldset, not a card.

## Components

### Nav item

- 30px tall, 12px horizontal padding, 8px gap.
- Left: 3px wide × 16px tall bullet rail. Hidden by default, becomes accent-colored when active.
- Inactive: text muted, no background.
- Hover: text primary, no background.
- Active: text primary, bullet visible, optional 0.04 alpha accent tint at the row level (so the row glows faintly, not a solid blue block).

### Search input

- 40px tall, 14px input. Border 1px hairline. Focus ring: 4px accent at 16% alpha + 1px accent border. No shadow in resting state.

### Segmented budget control

- Inline with the search input on the right. Three labels in a 1px hairline pill. Active segment uses accent text + faint accent tint background, never solid blue fill. Keyboard arrow keys navigate.

### Result card

- 16px vertical padding, 20px horizontal. 1px hairline. 12px radius.
- Layout: eyebrow citation (uppercase, faint), then title row (title + tabular score), then snippet paragraph, then a baseline meta row of inline tabular numerals separated by mid-dots.
- Score breakdown rendered as `semantic 0.71 · lexical 1.00 · overlap 1.00 · phrase 1.00 · compactness 1.00`. Numerals tabular. The accent appears only when one of the five sub-scores is the dominant signal — that one is bolded.

### Drop zone

- A bordered rectangle the user can drop folders or files into. Border becomes accent at 60% alpha when dragging over. Internal layout: subtle glyph, line of instruction, line of details, single primary button.

### Stat row (Library)

- Inline metric rows, not a grid of three cards. Pattern: large tabular number, label below in eyebrow type, separator hairline between rows. Aligned to a single column, reads top-down.

### Provider option (Settings)

- Vertical radio-card list. Each option: name (title), one-line cost/tradeoff (body), status (eyebrow with the active model and base url if relevant). Active option: accent left-rail bullet + accent border, no fill.

## Motion

- Section switch: 180ms fade + 6px translateY ease-out-quart on the entering panel.
- Result enter: 140ms fade per result, no stagger longer than 60ms total.
- Toast: 200ms fade.
- Hover state changes: 160ms color transitions; no transforms on hover except the dropzone (-2px y when dragging-over).
- `@media (prefers-reduced-motion: reduce)` disables all transforms and reduces fades to 0ms.

## Iconography

No icon library. The brand mark is a single monogram glyph (lowercase `m` in SF Pro Display, weighted, inside a 24×24 rounded square with a 1px hairline and a 12% accent fill). Section glyphs in the nav are single-character marks tuned visually: `·` for Search (a dot to suggest a target), `▢` for Library, `◐` for Settings. No emoji.

## States

- **Loading**: render the layout, fill cells with a 1×1 oklch placeholder at low chroma, no spinners under 400ms.
- **Empty Search** (with no docs indexed): three-line onboarding card with a single CTA pointing at the Library section.
- **Empty Search** (with docs indexed): muted hint listing scope chips ("title", "heading", "path") and one example query.
- **Error toast**: a single pill at the bottom, never blocking.

## Accessibility

- All interactive elements reachable by keyboard. Visible focus ring uses accent at 4px width and 25% alpha.
- Color contrast for text against its background ≥ 4.5:1 in both modes. Faint text used only for non-essential metadata.
- Prefers-reduced-motion respected.
- Hit targets ≥ 28px on density-tuned controls, ≥ 36px on touch-likely elements.
