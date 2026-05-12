import test from "node:test";
import assert from "node:assert/strict";
import { chunkMarkdown } from "../src/chunker.js";

test("chunks markdown by headings and preserves text", () => {
  const chunks = chunkMarkdown("# Title\n\nAlpha beta gamma.\n\n## Next\n\nDelta epsilon.");
  assert.equal(chunks.length, 2);
  assert.equal(chunks[0].heading, "Title");
  assert.match(chunks[1].text, /Delta/);
});
