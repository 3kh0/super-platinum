// Exercise the checker action that runs before an attachment dialog exists.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
const source = readFileSync(new URL("../src/desktop/scheduled/attachments/fixture/check.js", import.meta.url), "utf8");

async function selectTab({ settles = true, label = "Drafts" } = {}) {
  let clicks = 0;
  const tabs = ["Drafts 1", "Scheduled 1"].map((textContent, index) => ({
    textContent, selected: index === 1, disabled: false,
    getBoundingClientRect: () => ({left: 0, top: 0, right: 100, bottom: 32, width: 100, height: 32}),
    scrollIntoView() {}, focus() {}, contains() { return false; },
    click() { clicks++; if (settles) tabs.forEach(tab => { tab.selected = tab === this; }); },
  }));
  const context = vm.createContext({
    innerWidth: 668, innerHeight: 380,
    requestAnimationFrame: callback => callback(),
    document: {
      getElementById: () => null, // No dialog yet, as in the failing native case.
      elementFromPoint: () => tabs[0],
      querySelectorAll: selector => selector === "[role=tab]" ? tabs : tabs.filter(tab => tab.selected),
    },
  });
  vm.runInContext(source, context);
  const receipt = await context.scheduledAttachmentCheck("tab", {label});
  return {receipt, clicks};
}

const selected = await selectTab();
assert.equal(selected.receipt.ok, true);
assert.equal(selected.clicks, 1);
assert.equal(selected.receipt.selected.join(), "Drafts 1");
const stale = await selectTab({settles: false});
assert.equal(stale.receipt.ok, false); // A click alone cannot establish selection.
assert.equal(stale.clicks, 1);
await assert.rejects(selectTab({label: "Missing"}), /Unknown tab/);

// A returned owner can already match the retained Rust lease while its new
// ScheduledPanel has not reached the WebView. Readiness must never click.
let recoveryButtons = [];
let mounted = null;
let settle = () => {};
let recoveryClicks = 0;
const recovery = {
  textContent: "Review attachment changes", disabled: false,
  click() { recoveryClicks++; },
};
const recoveryContext = vm.createContext({
  requestAnimationFrame: callback => { settle(); callback(); },
  document: {
    getElementById: () => mounted,
    querySelectorAll: () => recoveryButtons,
  },
});
vm.runInContext(source, recoveryContext);
assert.equal((await recoveryContext.scheduledAttachmentCheck("resume-ready", {})).ok, false);
settle = () => { recoveryButtons = [recovery]; };
assert.equal((await recoveryContext.scheduledAttachmentCheck("resume-ready", {})).ok, true);
settle = () => {};
recovery.disabled = true;
assert.equal((await recoveryContext.scheduledAttachmentCheck("resume-ready", {})).ok, false);
recovery.disabled = false;
recoveryButtons = [recovery, {...recovery}];
assert.equal((await recoveryContext.scheduledAttachmentCheck("resume-ready", {})).ok, false);
recoveryButtons = [recovery];
mounted = {};
assert.equal((await recoveryContext.scheduledAttachmentCheck("resume-ready", {})).ok, false);
assert.equal(recoveryClicks, 0);
console.log("8 attachment checker regressions passed");
