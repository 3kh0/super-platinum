import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import test from "node:test";

const context = {};
runInNewContext(readFileSync(new URL("../src/desktop/drafts/remote_review_check.js", import.meta.url), "utf8"), context);
const delta = context.draftRemoteReviewScrollDelta;
const viewport = { top: 200, bottom: 340 };

test("visible control does not move the inventory or its ancestors", () => {
  assert.equal(delta({ top: 240, bottom: 272 }, viewport), 0);
});
test("a clipped control scrolls only the missing edge into view", () => {
  assert.equal(delta({ top: 180, bottom: 212 }, viewport), -20);
  assert.equal(delta({ top: 320, bottom: 352 }, viewport), 12);
});
test("centering a complete review row leaves equal space at both edges", () => {
  const row = { top: 450, bottom: 570 };
  const shift = delta(row, viewport, true);
  assert.equal(row.top - shift - viewport.top, 10);
  assert.equal(viewport.bottom - (row.bottom - shift), 10);
});
