// Local AI Memory — desktop frontend
// Talks to the Rust core via window.__TAURI__.core.invoke.

const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);

const els = {
  body: document.body,
  navItems: document.querySelectorAll(".nav-item"),
  panels: document.querySelectorAll(".panel"),
  statusPill: document.getElementById("status-pill"),
  statusPillText: document.querySelector("#status-pill .pill-text"),
  providerSummaryName: document.getElementById("provider-summary-name"),
  providerSummaryDetail: document.getElementById("provider-summary-detail"),
  searchInput: document.getElementById("search-input"),
  searchEmpty: document.getElementById("search-empty"),
  searchEmptyHint: document.getElementById("search-empty-hint"),
  searchResults: document.getElementById("search-results"),
  segments: document.querySelectorAll(".seg"),
  suggestions: document.querySelectorAll(".suggestion"),
  dropzone: document.getElementById("dropzone"),
  dropzoneBrowse: document.getElementById("dropzone-browse"),
  metaReady: document.getElementById("meta-ready"),
  metaError: document.getElementById("meta-error"),
  metaJobs: document.getElementById("meta-jobs"),
  metaChunks: document.getElementById("meta-chunks"),
  ingestLog: document.getElementById("ingest-log"),
  ingestCount: document.getElementById("ingest-count"),
  ingestProgress: document.getElementById("ingest-progress"),
  ingestBar: document.getElementById("ingest-bar"),
  ingestProgressText: document.getElementById("ingest-progress-text"),
  ingestProgressFile: document.getElementById("ingest-progress-file"),
  ingestCancel: document.getElementById("ingest-cancel"),
  resetLibrary: document.getElementById("reset-library"),
  watchedList: document.getElementById("watched-list"),
  watchedCount: document.getElementById("watched-count"),
  providerList: document.getElementById("provider-list"),
  providers: document.querySelectorAll(".provider"),
  parserDetail: document.getElementById("parser-detail"),
  parserEyebrow: document.getElementById("parser-eyebrow"),
  activeProviderEyebrow: document.getElementById("active-provider-eyebrow"),
  storeNetwork: document.getElementById("store-network"),
  toast: document.getElementById("toast"),
};

const state = {
  budget: "low",
  searchTimer: null,
  searchSeq: 0,
  ingestHistory: [],
  docCount: 0,
};

init().catch((err) => toast(`init failed: ${err}`));

async function init() {
  try { await invoke("app_init_store"); } catch (_) {}
  wireNav();
  wireSearch();
  wireLibrary();
  wireSettings();
  wireShortcuts();
  await refreshStatus();
  await refreshEmbeddings();
  await refreshParsers();
  await refreshWatched();
}

/* ---------------- nav + shortcuts ---------------- */

function wireNav() {
  els.navItems.forEach((item) => {
    item.addEventListener("click", () => switchSection(item.dataset.target));
  });
}

function wireShortcuts() {
  window.addEventListener("keydown", (event) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "k") {
      event.preventDefault();
      switchSection("search");
      els.searchInput.focus();
      els.searchInput.select();
      return;
    }
    if (event.metaKey || event.ctrlKey) {
      if (event.key === "1") { event.preventDefault(); switchSection("search"); }
      if (event.key === "2") { event.preventDefault(); switchSection("library"); }
      if (event.key === "3") { event.preventDefault(); switchSection("settings"); }
    }
  });
}

function switchSection(target) {
  els.body.dataset.section = target;
  els.navItems.forEach((item) => {
    const active = item.dataset.target === target;
    item.classList.toggle("is-active", active);
    item.setAttribute("aria-selected", active ? "true" : "false");
  });
  els.panels.forEach((panel) => {
    panel.hidden = panel.dataset.panel !== target;
  });
  if (target === "library") refreshStatus();
  if (target === "settings") { refreshEmbeddings(); refreshParsers(); }
  if (target === "search") setTimeout(() => els.searchInput.focus(), 60);
}

/* ---------------- search ---------------- */

function wireSearch() {
  els.searchInput.addEventListener("input", () => scheduleSearch());
  els.searchInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") { event.preventDefault(); runSearch(); }
    if (event.key === "Escape") { els.searchInput.value = ""; renderResults([]); }
  });
  els.segments.forEach((seg) => {
    seg.addEventListener("click", () => {
      state.budget = seg.dataset.budget;
      els.segments.forEach((s) => {
        const active = s === seg;
        s.classList.toggle("is-active", active);
        s.setAttribute("aria-selected", active ? "true" : "false");
      });
      if (els.searchInput.value.trim()) runSearch();
    });
  });
  els.suggestions.forEach((btn) => {
    btn.addEventListener("click", () => {
      els.searchInput.value = btn.dataset.query;
      runSearch();
      els.searchInput.focus();
    });
  });
}

function scheduleSearch() {
  clearTimeout(state.searchTimer);
  const query = els.searchInput.value.trim();
  if (!query) { renderResults([]); return; }
  state.searchTimer = setTimeout(runSearch, 220);
}

async function runSearch() {
  const query = els.searchInput.value.trim();
  if (!query) { renderResults([]); return; }
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
  els.searchResults.innerHTML = rows.map(renderResult).join("");
}

function renderResult(row) {
  const breakdown = row.score_breakdown || {};
  const semantic = num(breakdown.semantic);
  const lexical = num(breakdown.lexical);
  const overlap = num(breakdown.overlap);
  const phrase = num(breakdown.phrase);
  const compactness = num(breakdown.compactness);
  const dominant = dominantSignal({ semantic, lexical, overlap, phrase, compactness });
  const cite = buildCitation(row);
  return `
    <article class="result">
      <p class="result-citation">${escape(cite)}</p>
      <header class="result-head">
        <h3 class="result-title">${escape(row.title)}</h3>
        <span class="result-score"><b>${row.score.toFixed(2)}</b><span style="color:var(--text-faint)"> · score</span></span>
      </header>
      <p class="result-text">${escape(snippet(row.text))}</p>
      <div class="result-meta">
        ${metaItem("semantic", semantic, dominant)}
        <span class="meta-dot"></span>
        ${metaItem("lexical", lexical, dominant)}
        <span class="meta-dot"></span>
        ${metaItem("overlap", overlap, dominant)}
        <span class="meta-dot"></span>
        ${metaItem("phrase", phrase, dominant)}
        <span class="meta-dot"></span>
        ${metaItem("compactness", compactness, dominant)}
        <span class="meta-dot"></span>
        <span>${row.token_count} tok</span>
        <span class="meta-dot"></span>
        <span class="meta-path" title="${escape(row.path || "")}">${escape(shortPath(row.path))}</span>
      </div>
    </article>
  `;
}

function metaItem(label, value, dominant) {
  const strong = dominant === label;
  const cls = strong ? "meta-strong" : "";
  return `<span class="${cls}">${label} ${value.toFixed(2)}</span>`;
}

function num(v) {
  if (typeof v === "number") return v;
  if (v && typeof v === "object" && typeof v.toString === "function") return Number(v) || 0;
  return Number(v) || 0;
}

function dominantSignal(scores) {
  let best = null;
  let bestVal = -Infinity;
  for (const [k, v] of Object.entries(scores)) {
    if (v > bestVal) { bestVal = v; best = k; }
  }
  return bestVal > 0 ? best : null;
}

function buildCitation(row) {
  const bits = [];
  if (row.title) bits.push(row.title);
  if (row.page) bits.push(`page ${row.page}`);
  else if (row.slide) bits.push(`slide ${row.slide}`);
  else if (row.heading && row.heading !== row.title) bits.push(row.heading);
  return bits.join(" · ");
}

function snippet(text) {
  const clean = (text || "").replace(/\s+/g, " ").trim();
  return clean.length > 360 ? `${clean.slice(0, 360)}…` : clean;
}

function shortPath(path) {
  if (!path) return "";
  const parts = path.split(/[\\/]/);
  return parts.slice(-2).join("/");
}

/* ---------------- library ---------------- */

function wireLibrary() {
  els.dropzoneBrowse.addEventListener("click", () => {
    toast("Drag a folder onto the dropzone");
  });

  const evt = window.__TAURI__?.event;
  if (!evt?.listen) return;

  evt.listen("files-dropped", async (event) => {
    els.dropzone.classList.remove("is-hover");
    const paths = Array.isArray(event?.payload) ? event.payload : [];
    if (!paths.length) { toast("No file paths in drop"); return; }
    switchSection("library");
    for (const p of paths) {
      try { await invoke("app_watch_folder", { path: p }); } catch (_) {}
    }
    await refreshWatched();
  });

  evt.listen("watcher-ingest", (event) => {
    const p = event?.payload || {};
    const detail = p.error ? p.error : `${p.chunks ?? 0} chunks · ${p.source || "live"}`;
    state.ingestHistory = [{ file: p.file, status: p.status || "ready", detail }]
      .concat(state.ingestHistory).slice(0, 200);
    renderIngest();
    if (p.total && p.index) {
      showProgress(p.index, p.total, p.file || "");
    }
  });

  evt.listen("watcher-event", (event) => {
    const p = event?.payload || {};
    if (p.kind === "scan-complete") {
      hideProgress();
      refreshStatus();
    }
  });

  evt.listen("watcher-status", async () => { await refreshWatched(); });

  evt.listen("ingest-start", (event) => {
    const total = event?.payload?.total || 0;
    showProgress(0, total, "");
    state.ingestHistory = [];
    renderIngest();
  });

  evt.listen("ingest-progress", (event) => {
    const p = event?.payload || {};
    showProgress(p.index || 0, p.total || 0, p.file || "");
    const detail = p.error ? p.error : `${p.chunks ?? 0} chunks`;
    state.ingestHistory = [{ file: p.file, status: p.status || "ready", detail }]
      .concat(state.ingestHistory).slice(0, 200);
    renderIngest();
  });

  evt.listen("ingest-complete", async (event) => {
    const p = event?.payload || {};
    const completed = p.completed ?? p.total ?? 0;
    const total = p.total ?? 0;
    hideProgress();
    if (p.cancelled) {
      toast(`Cancelled · ${completed} / ${total} processed`);
    } else {
      toast(`${total} file${total === 1 ? "" : "s"} processed`);
    }
    await refreshStatus();
  });

  els.ingestCancel.addEventListener("click", async () => {
    els.ingestCancel.disabled = true;
    els.ingestCancel.textContent = "Cancelling…";
    try { await invoke("app_cancel_ingest"); } catch (_) {}
  });

  let resetArmed = false;
  let resetTimer = null;
  els.resetLibrary.addEventListener("click", async () => {
    if (!resetArmed) {
      resetArmed = true;
      els.resetLibrary.textContent = "Click again to confirm";
      els.resetLibrary.classList.add("is-arm");
      clearTimeout(resetTimer);
      resetTimer = setTimeout(() => {
        resetArmed = false;
        els.resetLibrary.textContent = "Reset library";
        els.resetLibrary.classList.remove("is-arm");
      }, 3000);
      return;
    }
    resetArmed = false;
    clearTimeout(resetTimer);
    els.resetLibrary.classList.remove("is-arm");
    els.resetLibrary.textContent = "Resetting…";
    els.resetLibrary.disabled = true;
    try {
      const result = await invoke("app_reset_library");
      state.ingestHistory = [];
      renderIngest();
      toast(`Library cleared · ${result?.documents ?? 0} docs removed`);
      await refreshStatus();
    } catch (err) {
      toast(String(err));
    } finally {
      els.resetLibrary.textContent = "Reset library";
      els.resetLibrary.disabled = false;
    }
  });
}

function showProgress(done, total, file) {
  if (!els.ingestProgress) return;
  els.ingestProgress.hidden = false;
  const pct = total > 0 ? Math.round((done / total) * 100) : 0;
  els.ingestBar.style.width = `${pct}%`;
  els.ingestProgressText.textContent = `${done} / ${total}`;
  els.ingestProgressFile.textContent = file ? shortPath(file) : "";
  els.ingestCancel.disabled = false;
  els.ingestCancel.textContent = "Cancel";
}

function hideProgress() {
  if (!els.ingestProgress) return;
  els.ingestProgress.hidden = true;
  els.ingestBar.style.width = "0%";
  els.ingestCancel.disabled = false;
  els.ingestCancel.textContent = "Cancel";
}

async function ingest(paths) {
  try {
    const total = await invoke("app_add_paths", { paths });
    if (typeof total === "number" && total > 0) {
      showProgress(0, total, "");
    }
  } catch (err) {
    toast(String(err));
  }
}

async function refreshWatched() {
  if (!els.watchedList) return;
  try {
    const list = await invoke("app_watched_folders");
    renderWatched(Array.isArray(list) ? list : []);
  } catch (err) {
    els.watchedList.innerHTML = `<p class="ingest-empty">error: ${escape(String(err))}</p>`;
  }
}

function renderWatched(paths) {
  els.watchedCount.textContent = String(paths.length);
  if (paths.length === 0) {
    els.watchedList.innerHTML = `<p class="ingest-empty">No folders are being watched.</p>`;
    return;
  }
  els.watchedList.innerHTML = paths.map((p) => `
    <div class="watched-row">
      <span class="rail" aria-hidden="true"></span>
      <span class="path" title="${escape(p)}">${escape(p)}</span>
      <button class="unwatch" data-path="${escape(p)}" type="button">Unwatch</button>
    </div>
  `).join("");
  els.watchedList.querySelectorAll(".unwatch").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const path = btn.dataset.path;
      btn.disabled = true;
      try {
        await invoke("app_unwatch_folder", { path });
        await refreshWatched();
        toast(`Unwatched ${shortPath(path)}`);
      } catch (err) {
        btn.disabled = false;
        toast(String(err));
      }
    });
  });
}

function renderIngest() {
  if (state.ingestHistory.length === 0) {
    els.ingestLog.innerHTML = `<p class="ingest-empty">No recent ingest in this session.</p>`;
    els.ingestCount.textContent = "";
    return;
  }
  els.ingestCount.textContent = `${state.ingestHistory.length} event${state.ingestHistory.length === 1 ? "" : "s"}`;
  els.ingestLog.innerHTML = state.ingestHistory.map((row) => {
    const cls = row.status === "error" ? "row error" : "row";
    return `
      <div class="${cls}">
        <span class="status">${escape(row.status)}</span>
        <span class="file">${escape(row.file)}</span>
        <span class="detail">${escape(row.detail || "")}</span>
      </div>
    `;
  }).join("");
}

/* ---------------- settings ---------------- */

function wireSettings() {
  els.providers.forEach((btn) => {
    btn.addEventListener("click", async () => {
      const provider = btn.dataset.provider;
      els.providers.forEach((b) => b.classList.toggle("is-active", b === btn));
      try {
        const view = await invoke("app_set_embedding", {
          provider, model: null, baseUrl: null, dimensions: null,
        });
        renderEmbeddings(view);
        toast(`Embedding provider set to ${provider}. Reindex required.`);
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
    state.docCount = ready;
    if (els.metaReady) els.metaReady.textContent = ready;
    if (els.metaError) els.metaError.textContent = errors;
    if (els.metaJobs) els.metaJobs.textContent = jobs;
    if (els.metaChunks) els.metaChunks.textContent = ready * 1; /* approximation; backend exposes counts later */
    updateEmptyHint(ready);
    updateStatusPill(ready);
  } catch (err) {
    if (els.statusPillText) els.statusPillText.textContent = "Store unavailable";
  }
}

function updateEmptyHint(ready) {
  if (!els.searchEmptyHint) return;
  if (ready === 0) {
    els.searchEmptyHint.innerHTML = `No documents indexed yet. Open <b>Library</b> and drop a folder.`;
  } else {
    els.searchEmptyHint.innerHTML = `Searching across <b>${ready}</b> document${ready === 1 ? "" : "s"}. Type a phrase or pick a suggestion.`;
  }
}

function updateStatusPill(ready) {
  els.statusPillText.textContent = ready > 0 ? `${ready} doc${ready === 1 ? "" : "s"} · offline` : "Local · offline";
}

async function refreshEmbeddings() {
  try {
    const view = await invoke("app_embeddings");
    renderEmbeddings(view);
  } catch (err) {
    if (els.activeProviderEyebrow) els.activeProviderEyebrow.textContent = `error · ${err}`;
  }
}

function renderEmbeddings(view) {
  const active = view.active || {};
  els.providers.forEach((btn) =>
    btn.classList.toggle("is-active", btn.dataset.provider === active.provider)
  );
  if (els.activeProviderEyebrow) {
    els.activeProviderEyebrow.textContent = `current · ${active.provider || "—"}`;
  }
  if (els.providerSummaryName) {
    els.providerSummaryName.textContent = active.provider || "—";
  }
  if (els.providerSummaryDetail) {
    const model = active.model || "";
    els.providerSummaryDetail.textContent = model;
  }
  const isCloud = active.provider && active.provider !== "local" && active.provider !== "ollama";
  if (els.statusPill) {
    if (isCloud) {
      els.statusPill.setAttribute("data-state", "cloud");
      els.statusPillText.textContent = `${active.provider} · cloud`;
    } else {
      els.statusPill.removeAttribute("data-state");
      updateStatusPill(state.docCount);
    }
  }
  if (els.storeNetwork) {
    els.storeNetwork.textContent = isCloud
      ? `Outbound to ${active.base_url || active.provider} on query and reindex`
      : "Offline by default";
  }
}

async function refreshParsers() {
  try {
    const parsers = await invoke("app_parsers");
    renderParsers(parsers);
  } catch (err) {
    els.parserDetail.innerHTML = `<p class="parser-note">error: ${escape(String(err))}</p>`;
    els.parserEyebrow.textContent = "error";
  }
}

function renderParsers(parsers) {
  const engines = parsers?.engines || {};
  const ready = parsers?.ready === true;
  els.parserEyebrow.textContent = ready ? "ready" : "not ready";
  const knownEngines = ["docling", "markitdown", "pypdf", "python-docx", "openpyxl", "python-pptx"];
  const rows = knownEngines.map((name) => {
    const present = !!engines[name];
    const cls = present ? "parser-engine is-ready" : "parser-engine is-missing";
    const status = present ? "available" : "not installed";
    return `
      <div class="${cls}">
        <span class="dot"></span>
        <span class="name">${escape(name)}</span>
        <span class="status">${escape(status)}</span>
      </div>
    `;
  }).join("");
  const note = parsers?.message
    ? `<p class="parser-note">${escape(parsers.message)}</p>`
    : ready
      ? ""
      : `<p class="parser-note">No Python parser detected. Native extensions (.md, .txt, .csv, .json, .html) still work.</p>`;
  els.parserDetail.innerHTML = rows + note;
}

/* ---------------- toast ---------------- */

function toast(message) {
  els.toast.textContent = message;
  els.toast.hidden = false;
  requestAnimationFrame(() => els.toast.classList.add("is-visible"));
  clearTimeout(toast._timer);
  toast._timer = setTimeout(() => {
    els.toast.classList.remove("is-visible");
    setTimeout(() => { els.toast.hidden = true; }, 250);
  }, 2400);
}

function escape(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}
