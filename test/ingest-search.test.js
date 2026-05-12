import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { addPath, initStore, searchMemory, status } from "../src/indexer.js";

test("ingests a markdown file and finds it", async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mem-test-"));
  const store = path.join(dir, ".memoria");
  const file = path.join(dir, "strategy.md");
  fs.writeFileSync(file, "# Strategy\n\nEnterprise pricing uses annual contracts and renewal notices.", "utf8");

  initStore(store);
  const [result] = await addPath(file, { base: store });
  assert.equal(result.status, "ready");

  const results = await searchMemory("enterprise pricing", { base: store, budget: "low" });
  assert.ok(results.length >= 1);
  assert.match(results[0].text, /Enterprise pricing/i);

  const state = status({ base: store });
  assert.deepEqual(state.documents, [{ status: "ready", count: 1 }]);
});

test("reranks exact lexical matches above nearby semantic matches", async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mem-rank-test-"));
  const store = path.join(dir, ".memoria");
  const exact = path.join(dir, "pricing.md");
  const nearby = path.join(dir, "sales.md");
  fs.writeFileSync(exact, "# Enterprise Pricing\n\nEnterprise pricing uses renewal notices for seat tiers.", "utf8");
  fs.writeFileSync(nearby, "# Sales Motion\n\nCustomer contracts include annual plans and account expansion.", "utf8");

  initStore(store);
  await addPath(dir, { base: store });

  const results = await searchMemory("enterprise pricing renewal", { base: store, budget: "low" });
  assert.ok(results.length >= 2);
  assert.equal(results[0].title, "pricing.md");
  assert.equal(typeof results[0].score_breakdown.semantic, "number");
  assert.ok(results[0].score >= results[1].score);
});
