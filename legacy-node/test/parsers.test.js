import test from "node:test";
import assert from "node:assert/strict";
import { parserStatus } from "../src/extractors.js";

test("reports parser status without throwing", () => {
  const status = parserStatus();
  assert.equal(typeof status.ready, "boolean");
  assert.equal(typeof status.engines, "object");
});
