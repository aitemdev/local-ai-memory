import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const textExtensions = new Set([".md", ".txt", ".csv", ".tsv", ".json", ".html", ".htm"]);
const plannedBinaryExtensions = new Set([".pdf", ".docx", ".pptx", ".xlsx", ".png", ".jpg", ".jpeg", ".tiff", ".bmp", ".webp"]);
const parserScript = path.resolve("tools", "parse_document.py");

export function supportedExtensions() {
  return [...textExtensions, ...plannedBinaryExtensions].sort();
}

export function parserStatus() {
  const python = resolvePython();
  if (!python) {
    return {
      python: null,
      engines: {},
      ready: false,
      message: "No Python executable found. Set MEM_PYTHON to enable Docling/MarkItDown parsers."
    };
  }
  const probe = spawnSync(python, [parserScript, "--probe"], {
    cwd: process.cwd(),
    encoding: "utf8",
    windowsHide: true
  });
  if (probe.status !== 0) {
    return {
      python,
      engines: {},
      ready: false,
      message: probe.stderr?.trim() || probe.stdout?.trim() || "Parser probe failed."
    };
  }
  return JSON.parse(probe.stdout);
}

export function extractDocument(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  const title = path.basename(filePath);
  if (textExtensions.has(ext)) {
    const raw = fs.readFileSync(filePath, "utf8");
    const markdown = normalizeTextToMarkdown(raw, ext, title);
    return {
      parser: "native-text",
      title,
      type: ext.slice(1) || "text",
      markdown,
      structured: {
        title,
        type: ext.slice(1) || "text",
        sections: [{ kind: "document", heading: title, text: markdown }]
      }
    };
  }

  if (plannedBinaryExtensions.has(ext)) {
    return extractWithPython(filePath, title, ext);
  }

  throw new Error(`Unsupported file extension: ${ext || "(none)"}`);
}

function extractWithPython(filePath, title, ext) {
  const python = resolvePython();
  if (!python) {
    throw new Error("Python parser unavailable. Install Python or set MEM_PYTHON to a Python executable with Docling/MarkItDown.");
  }

  const result = spawnSync(python, [parserScript, filePath], {
    cwd: process.cwd(),
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    windowsHide: true
  });
  if (result.status !== 0) {
    throw new Error((result.stderr || result.stdout || "").trim() || `Parser failed for ${ext}`);
  }

  const parsed = JSON.parse(result.stdout);
  const markdown = parsed.markdown?.trim();
  if (!markdown) throw new Error(`Parser produced no Markdown for ${title}`);
  return {
    parser: parsed.parser,
    title: parsed.title || title,
    type: parsed.type || ext.slice(1),
    markdown,
    structured: parsed.structured || {
      title: parsed.title || title,
      type: parsed.type || ext.slice(1),
      sections: [{ kind: "document", heading: parsed.title || title, text: markdown }]
    }
  };
}

function resolvePython() {
  const candidates = [
    process.env.MEM_PYTHON,
    "C:\\Users\\abelj\\.cache\\codex-runtimes\\codex-primary-runtime\\dependencies\\python\\python.exe",
    "python",
    "py"
  ].filter(Boolean);

  for (const candidate of candidates) {
    const check = spawnSync(candidate, ["--version"], {
      encoding: "utf8",
      windowsHide: true
    });
    if (check.status === 0) return candidate;
  }
  return null;
}

function normalizeTextToMarkdown(raw, ext, title) {
  const trimmed = raw.replace(/\r\n/g, "\n").trim();
  if (ext === ".md") return trimmed;
  if (ext === ".csv" || ext === ".tsv") {
    const delimiter = ext === ".tsv" ? "\t" : ",";
    const lines = trimmed.split("\n").filter(Boolean);
    if (lines.length === 0) return `# ${title}\n`;
    const cells = lines.map((line) => line.split(delimiter).map((cell) => cell.trim()));
    const header = cells[0];
    const separator = header.map(() => "---");
    const rows = cells.slice(1);
    return [
      `# ${title}`,
      "",
      `| ${header.join(" | ")} |`,
      `| ${separator.join(" | ")} |`,
      ...rows.map((row) => `| ${row.join(" | ")} |`)
    ].join("\n");
  }
  if (ext === ".html" || ext === ".htm") {
    return `# ${title}\n\n${trimmed
      .replace(/<script[\s\S]*?<\/script>/gi, "")
      .replace(/<style[\s\S]*?<\/style>/gi, "")
      .replace(/<\/(h1|h2|h3|p|li|tr)>/gi, "\n")
      .replace(/<[^>]+>/g, " ")
      .replace(/[ \t]+/g, " ")
      .trim()}`;
  }
  return `# ${title}\n\n${trimmed}`;
}
