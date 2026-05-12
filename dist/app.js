const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

const els = {
  body: document.body,
  navItems: document.querySelectorAll(".nav-item"),
  panels: document.querySelectorAll(".panel"),
  statusPill: document.getElementById("status-pill"),
  searchInput: document.getElementById("search-input"),
  searchEmpty: document.getElementById("search-empty"),
  searchResults: document.getElementById("search-results"),
  budgetChips: document.querySelectorAll(".chip"),
  dropzone: document.getElementById("dropzone"),
  dropzoneBrowse: document.getElementById("dropzone-browse"),
  metaReady: document.getElementById("meta-ready"),
  metaError: document.getElementById("meta-error"),
  metaJobs: document.getElementById("meta-jobs"),
  ingestLog: document.getElementById("ingest-log"),
  providers: document.querySelectorAll(".provider"),
  embeddingDetail: document.getElementById("embedding-detail"),
  parserDetail: document.getElementById("parser-detail"),
  toast: document.getElementById("toast"),
};

const state = {
  budget: "low",
  searchTimer: null,
  searchSeq: 0,
};

init().catch((err) => toast(`init failed: ${err}`));

async function init() {
  await invoke("app_init_store").catch(() => null);
  wireNav();
  wireSearch();
  wireLibrary();
  wireSettings();
  await refreshStatus();
  await refreshEmbeddings();
  await refreshParsers();
}

function wireNav() {
  els.navItems.forEach((item) => {
    item.addEventListener("click", () => switchSection(item.dataset.target));
  });
}

function switchSection(target) {
  els.body.dataset.section = target;
  els.navItems.forEach((item) => item.classList.toggle("is-active", item.dataset.target === target));
  els.panels.forEach((panel) => {
    panel.hidden = panel.dataset.panel !== target;
  });
  if (target === "library") refreshStatus();
  if (target === "settings") {
    refreshEmbeddings();
    refreshParsers();
  }
}

function wireSearch() {
  els.searchInput.addEventListener("input", () => scheduleSearch());
  els.searchInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      runSearch();
    }
  });
  els.budgetChips.forEach((chip) => {
    chip.addEventListener("click", () => {
      state.budget = chip.dataset.budget;
      els.budgetChips.forEach((c) => c.classList.toggle("is-active", c === chip));
      if (els.searchInput.value.trim()) runSearch();
    });
  });
}

function scheduleSearch() {
  clearTimeout(state.searchTimer);
  const query = els.searchInput.value.trim();
  if (!query) {
    renderResults([]);
    return;
  }
  state.searchTimer = setTimeout(runSearch, 220);
}

async function runSearch() {
  const query = els.searchInput.value.trim();
  if (!query) {
    renderResults([]);
    return;
  }
  const seq = ++state.searchSeq;
  try {
    const results = await invoke("app_search", { query, budget: state.budget, limit: null });
    if (seq !== state.searchSeq) return;
    renderResults(results);
  } catch (err) {
    toast(String(err));
  }
}

function renderResults(rows) {
  if (!rows || rows.length === 0) {
    els.searchResults.hidden = true;
    els.searchEmpty.hidden = false;
    els.searchResults.innerHTML = "";
    return;
  }
  els.searchEmpty.hidden = true;
  els.searchResults.hidden = false;
  els.searchResults.innerHTML = rows
    .map(
      (row, i) => `
      <article class="result-card">
        <header class="result-head">
          <div>
            <div class="result-title">${escape(row.title)}</div>
            <div class="result-citation">${escape(row.citation)}</div>
          </div>
          <div class="result-score">${row.score.toFixed(3)}</div>
        </header>
        <p class="result-text">${escape(snippet(row.text))}</p>
        <div class="result-meta">
          <span class="tag">#${i + 1}</span>
          <span class="tag">tokens ${row.token_count}</span>
          <span class="tag">semantic ${(row.score_breakdown?.semantic ?? 0).toFixed(2)}</span>
          <span class="tag">lexical ${(row.score_breakdown?.lexical ?? 0).toFixed(2)}</span>
          <span class="tag">${escape(shortPath(row.path))}</span>
        </div>
      </article>
    `
    )
    .join("");
}

function snippet(text) {
  const clean = (text || "").replace(/\s+/g, " ").trim();
  return clean.length > 320 ? `${clean.slice(0, 320)}…` : clean;
}

function shortPath(path) {
  if (!path) return "";
  const parts = path.split(/[\\/]/);
  return parts.slice(-2).join("/");
}

function wireLibrary() {
  els.dropzoneBrowse.addEventListener("click", () => {
    toast("Use drag-drop for now");
  });
  ["dragenter", "dragover"].forEach((event) =>
    els.dropzone.addEventListener(event, (e) => {
      e.preventDefault();
      els.dropzone.classList.add("is-hover");
    })
  );
  ["dragleave", "drop"].forEach((event) =>
    els.dropzone.addEventListener(event, (e) => {
      e.preventDefault();
      els.dropzone.classList.remove("is-hover");
    })
  );
  els.dropzone.addEventListener("drop", async (event) => {
    const paths = collectPaths(event);
    if (!paths.length) {
      toast("Could not read file paths");
      return;
    }
    await ingest(paths);
  });
}

function collectPaths(event) {
  const out = [];
  if (event.dataTransfer?.items) {
    for (const item of event.dataTransfer.items) {
      const file = item.getAsFile?.();
      if (file && file.path) out.push(file.path);
    }
  }
  if (out.length === 0 && event.dataTransfer?.files) {
    for (const file of event.dataTransfer.files) {
      if (file.path) out.push(file.path);
    }
  }
  return out;
}

async function ingest(paths) {
  els.ingestLog.innerHTML = paths
    .map((p) => `<div class="row" data-path="${escape(p)}">indexing ${escape(p)}…</div>`)
    .join("");
  try {
    const results = await invoke("app_add_paths", { paths });
    els.ingestLog.innerHTML = results
      .map((row) => {
        const status = row.status || "ready";
        const cls = status === "error" ? "row error" : "row";
        const detail = row.error ? ` · ${row.error}` : ` · ${row.chunks ?? 0} chunks`;
        return `<div class="${cls}">${escape(row.file)} → ${escape(status)}${escape(detail)}</div>`;
      })
      .join("");
    toast(`${results.length} file(s) ingested`);
    await refreshStatus();
  } catch (err) {
    toast(String(err));
  }
}

function wireSettings() {
  els.providers.forEach((btn) => {
    btn.addEventListener("click", async () => {
      const provider = btn.dataset.provider;
      els.providers.forEach((b) => b.classList.toggle("is-active", b === btn));
      try {
        const view = await invoke("app_set_embedding", {
          provider,
          model: null,
          baseUrl: null,
          dimensions: null,
        });
        renderEmbeddings(view);
        toast(`Provider: ${provider}. Reindex required.`);
      } catch (err) {
        toast(String(err));
      }
    });
  });
}

async function refreshStatus() {
  try {
    const status = await invoke("app_status");
    const docs = status.documents || [];
    const ready = docs.find((d) => d.status === "ready")?.count ?? 0;
    const errors = docs.find((d) => d.status === "error")?.count ?? 0;
    const jobs = (status.jobs || []).reduce((sum, j) => sum + (j.count ?? 0), 0);
    els.metaReady.textContent = ready;
    els.metaError.textContent = errors;
    els.metaJobs.textContent = jobs;
    els.statusPill.textContent = `${ready} document${ready === 1 ? "" : "s"}`;
  } catch (err) {
    els.statusPill.textContent = "no store";
  }
}

async function refreshEmbeddings() {
  try {
    const view = await invoke("app_embeddings");
    renderEmbeddings(view);
  } catch (err) {
    els.embeddingDetail.textContent = `error: ${err}`;
  }
}

function renderEmbeddings(view) {
  const { active } = view;
  els.providers.forEach((btn) =>
    btn.classList.toggle("is-active", btn.dataset.provider === active.provider)
  );
  els.embeddingDetail.textContent = JSON.stringify(view, null, 2);
}

async function refreshParsers() {
  try {
    const parsers = await invoke("app_parsers");
    els.parserDetail.textContent = JSON.stringify(parsers, null, 2);
  } catch (err) {
    els.parserDetail.textContent = `error: ${err}`;
  }
}

function toast(message) {
  els.toast.textContent = message;
  els.toast.hidden = false;
  els.toast.classList.add("is-visible");
  clearTimeout(toast._timer);
  toast._timer = setTimeout(() => {
    els.toast.classList.remove("is-visible");
    setTimeout(() => (els.toast.hidden = true), 200);
  }, 2400);
}

function escape(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
