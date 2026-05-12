const statusEl = document.querySelector("#status");
const resultsEl = document.querySelector("#results");

document.querySelector("#refresh").addEventListener("click", refreshStatus);
document.querySelector("#search").addEventListener("click", runSearch);
document.querySelector("#query").addEventListener("keydown", (event) => {
  if (event.key === "Enter") runSearch();
});
document.querySelector("#add").addEventListener("click", addPath);

refreshStatus();

async function refreshStatus() {
  const response = await fetch("/api/status");
  statusEl.textContent = JSON.stringify(await response.json(), null, 2);
}

async function runSearch() {
  const query = document.querySelector("#query").value.trim();
  const budget = document.querySelector("#budget").value;
  if (!query) return;
  const response = await fetch(`/api/search?q=${encodeURIComponent(query)}&budget=${encodeURIComponent(budget)}`);
  const results = await response.json();
  resultsEl.innerHTML = results.map((result) => `
    <article class="result">
      <strong>${escapeHtml(result.citation)} · ${Number(result.score).toFixed(3)}</strong>
      <p>${escapeHtml(result.text.slice(0, 420))}${result.text.length > 420 ? "..." : ""}</p>
      <code>${escapeHtml(result.chunk_id)}</code>
    </article>
  `).join("") || "<p>No results.</p>";
}

async function addPath() {
  const path = document.querySelector("#path").value.trim();
  if (!path) return;
  resultsEl.innerHTML = "<p>Indexing...</p>";
  const response = await fetch("/api/add", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ path })
  });
  resultsEl.innerHTML = `<pre>${escapeHtml(JSON.stringify(await response.json(), null, 2))}</pre>`;
  await refreshStatus();
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#039;"
  }[char]));
}
