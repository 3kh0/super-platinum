"use strict";
// Model-only corpus. Fake trusted flags/permits are never browser/NativeAck proof.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const sandbox = { module: { exports: {} } };
vm.runInNewContext(fs.readFileSync(path.join(__dirname, "bridge.js"), "utf8"), sandbox,
    { filename: "thread-copy-bridge.js" });
const { runThreadCopyControl } = sandbox.module.exports;
const tick = () => new Promise(resolve => setImmediate(resolve));
const URL = "https://example.slack.com/archives/C12345678/p1710000001000001";
const options = { id: 11, mount: 7, nonce: "original-control", scope: "original-owner", hostId: "host", buttonId: "button" };

class Events {
    constructor() { this.listeners = new Map(); }
    addEventListener(kind, handler) {
        if (!this.listeners.has(kind)) this.listeners.set(kind, new Set());
        this.listeners.get(kind).add(handler);
    }
    removeEventListener(kind, handler) { this.listeners.get(kind)?.delete(handler); }
    dispatch(kind, data = {}) {
        const event = { currentTarget: this, target: this, isTrusted: true, preventDefault() {}, ...data };
        for (const handler of [...(this.listeners.get(kind) || [])]) handler(event);
    }
}
class Element extends Events {
    constructor(document, id, parent, tagName = "DIV") {
        super(); this.ownerDocument = document; this.id = id; this.parentElement = parent;
        this.tagName = tagName; this.type = tagName === "BUTTON" ? "button" : "";
        this.isConnected = true; this.hidden = false; this.inert = false; this.disabled = false;
        this.dataset = {}; this.attributes = new Map(); this.rects = [{ width: 80, height: 24 }];
        this.style = { display: "block", visibility: "visible", contentVisibility: "visible", opacity: "1" };
    }
    contains(node) { for (; node; node = node.parentElement) if (node === this) return true; return false; }
    getClientRects() { return this.rects; }
    hasAttribute(key) { return this.attributes.has(key) || (key === "disabled" && this.disabled); }
    getAttribute(key) { return this.attributes.get(key) ?? null; }
    matches(selector) {
        assert.equal(selector, ":disabled");
        for (let node = this; node; node = node.parentElement) if (node.disabled) return true;
        return false;
    }
}
function fixture({ initialDisabled = false, writer = () => Promise.resolve(), unsupported = false } = {}) {
    const document = new Events();
    document.hidden = false; document.visibilityState = "visible"; document.focused = true;
    document.hasFocus = () => document.focused;
    const body = new Element(document, "body", null);
    const host = new Element(document, options.hostId, body);
    const button = new Element(document, options.buttonId, host, "BUTTON");
    button.disabled = initialDisabled;
    host.dataset = { threadCopyScope: options.scope, threadCopyIntent: String(options.id), threadCopyMount: String(options.mount) };
    const nodes = new Map([[host.id, host], [button.id, button]]);
    document.getElementById = id => nodes.get(id) || null;
    document.activeElement = button;
    const environment = new Events();
    environment.window = environment; environment.parent = environment; environment.top = environment;
    environment.frameElement = null; environment.document = document; document.defaultView = environment;
    environment.Promise = Promise;
    environment.getComputedStyle = node => node.style;
    const writes = [];
    environment.navigator = { userActivation: { isActive: true }, clipboard: { writeText(text) { writes.push(text); return writer(text); } } };
    if (unsupported) delete environment.navigator.clipboard;
    const observers = [];
    environment.MutationObserver = class {
        constructor(callback) { this.callback = callback; this.disconnected = false; observers.push(this); }
        observe() {}
        disconnect() { this.disconnected = true; }
    };
    const sent = [], replies = [], waiters = [];
    let failReceive = false;
    const bridge = {
        send(message) {
            const keys = Object.keys(message).sort();
            assert.deepEqual(keys, (message.kind === "settled" ? ["kind", "id", "mount", "nonce", "seq", "outcome"] : ["kind", "id", "mount", "nonce", "seq"]).sort());
            sent.push(JSON.parse(JSON.stringify(message)));
        },
        recv() {
            if (failReceive) return Promise.reject(new Error("model receive lost"));
            if (replies.length) return Promise.resolve(replies.shift());
            return new Promise(resolve => waiters.push(resolve));
        },
    };
    const f = { document, environment, body, host, button, nodes, observers, sent, writes, bridge,
        mutate() { for (const observer of observers) if (!observer.disconnected) observer.callback([]); },
        click(data) { button.dispatch("click", data); },
        reply(value) { const waiter = waiters.shift(); if (waiter) waiter(value); else replies.push(value); },
        response(kind, seq = 1, extra = {}) { return { kind, id: options.id, mount: options.mount, nonce: options.nonce, seq, ...extra }; },
        failReceive() { failReceive = true; },
    };
    f.running = runThreadCopyControl(options, environment, bridge);
    return f;
}
async function mounted(f) { await tick(); assert.deepEqual(f.sent.map(m => m.kind), ["mounted"]); assert.equal(f.sent[0].seq, 0); }
async function pressed(f, seq = 1) { f.click(); await tick(); assert.equal(f.sent.at(-1).kind, "pressed"); assert.equal(f.sent.at(-1).seq, seq); }
async function permit(f, seq = 1) { f.reply(f.response("permit", seq, { url: URL })); await tick(); }
async function ack(f, seq = 1) { f.reply(f.response("ack", seq)); await tick(); }
async function stop(f) { f.host.isConnected = false; f.mutate(); await tick(); await f.running; }
function settled(f, outcome, seq = 1) {
    const message = f.sent.at(-1); assert.equal(message.kind, "settled"); assert.equal(message.seq, seq); assert.equal(message.outcome, outcome);
}
const tests = [];
function test(name, body) { tests.push([name, body]); }

test("exact success and two fresh gestures only after ack", async () => {
    const f = fixture(); await mounted(f); await pressed(f); await permit(f);
    assert.deepEqual(f.writes, [URL]); settled(f, "fulfilled");
    f.click(); await tick(); assert.equal(f.writes.length, 1); assert.equal(f.sent.length, 3);
    await ack(f); await tick(); assert.equal(f.writes.length, 1);
    await pressed(f, 2); await permit(f, 2); settled(f, "fulfilled", 2); await ack(f, 2);
    assert.deepEqual(f.writes, [URL, URL]); await stop(f);
});
test("initial disabled button mounts quietly until Rust enables it", async () => {
    const f = fixture({ initialDisabled: true }); await mounted(f); f.click(); await tick();
    assert.equal(f.sent.length, 1); f.button.disabled = false; f.mutate(); await pressed(f);
    await permit(f); settled(f, "fulfilled"); await ack(f); await stop(f);
});
test("synthetic, inactive, unfocused and foreign-target events never press", async () => {
    const f = fixture(); await mounted(f);
    f.click({ isTrusted: false }); f.document.focused = false; f.click(); f.document.focused = true;
    f.document.activeElement = f.host; f.click(); f.document.activeElement = f.button;
    f.environment.navigator.userActivation.isActive = false; f.click(); f.environment.navigator.userActivation.isActive = true;
    f.click({ target: new Element({}, "foreign", f.button) });
    f.click({ currentTarget: f.host }); await tick();
    assert.equal(f.sent.length, 1); assert.equal(f.writes.length, 0); await stop(f);
});
test("identity, hidden, inert, disabled and zero-size mutations retire before press", async () => {
    const changes = [
        f => { f.host.dataset.threadCopyScope = "foreign"; },
        f => { f.host.dataset.threadCopyIntent = "12"; },
        f => { f.host.dataset.threadCopyMount = "8"; },
        f => { f.nodes.set(options.buttonId, new Element(f.document, options.buttonId, f.host, "BUTTON")); },
        f => { f.host.hidden = true; }, f => { f.body.inert = true; },
        f => { f.button.disabled = true; }, f => { f.button.rects = [{ width: 0, height: 24 }]; },
        f => { f.host.style.opacity = "0"; }, f => { f.environment.parent = {}; },
    ];
    for (const change of changes) {
        const f = fixture(); await mounted(f); change(f); f.mutate(); f.click(); await tick(); await f.running;
        assert.deepEqual(f.sent.map(m => m.kind), ["mounted", "retired"]); assert.equal(f.sent[1].seq, 0);
        assert.equal(f.writes.length, 0); assert(f.observers.every(o => o.disconnected));
    }
});
test("activation/focus/provider/frame loss between press and permit is not_requested", async () => {
    const changes = [
        f => { f.environment.navigator.userActivation.isActive = false; },
        f => { f.document.focused = false; }, f => { f.document.activeElement = f.host; },
        f => { f.environment.navigator.clipboard = { writeText() { throw new Error("foreign clipboard"); } }; },
        f => { f.environment.navigator.clipboard.writeText = () => { throw new Error("foreign method"); }; },
        f => { f.environment.top = {}; }, f => { f.environment.document = {}; },
    ];
    for (const change of changes) {
        const f = fixture(); await mounted(f); await pressed(f); change(f); await permit(f);
        settled(f, "not_requested"); assert.equal(f.writes.length, 0); await ack(f); await stop(f);
    }
});
test("one in-flight press drops all queued peer gestures", async () => {
    let resolve; const f = fixture({ writer: () => new Promise(r => { resolve = r; }) });
    await mounted(f); await pressed(f); f.click(); f.click(); await tick();
    assert.equal(f.sent.filter(m => m.kind === "pressed").length, 1);
    await permit(f); f.click(); f.click(); await tick(); assert.equal(f.writes.length, 1);
    resolve(); await tick(); settled(f, "fulfilled"); f.click(); await tick();
    assert.equal(f.sent.filter(m => m.kind === "pressed").length, 1); await ack(f); await stop(f);
});
test("deny settles not_requested and cannot rearm before matching ack", async () => {
    const f = fixture(); await mounted(f); await pressed(f); f.reply(f.response("deny")); await tick();
    settled(f, "not_requested"); f.click(); await tick(); assert.equal(f.sent.length, 3);
    assert.equal(f.writes.length, 0); await ack(f); await stop(f);
});
test("actual rejected Promise is rejected, synchronous/thenable calls are unknown", async () => {
    for (const [writer, outcome] of [
        [() => Promise.reject(new Error("denied")), "rejected"],
        [() => { throw new Error("ambiguous call"); }, "unknown"],
        [() => undefined, "unknown"],
        [() => ({ then() { throw new Error("must not invoke thenable"); } }), "unknown"],
    ]) {
        const f = fixture({ writer }); await mounted(f); await pressed(f); await permit(f);
        settled(f, outcome); assert.deepEqual(f.writes, [URL]); await ack(f); await stop(f);
    }
});
test("held issued Promise drains through node retirement before retired event", async () => {
    let resolve; const f = fixture({ writer: () => new Promise(r => { resolve = r; }) });
    await mounted(f); await pressed(f); await permit(f); assert.deepEqual(f.writes, [URL]);
    f.host.isConnected = false; f.mutate(); f.click(); await tick();
    assert.deepEqual(f.sent.map(m => m.kind), ["mounted", "pressed"]);
    resolve(); await tick(); settled(f, "fulfilled"); assert.equal(f.sent.filter(m => m.kind === "retired").length, 0);
    await ack(f); await f.running; assert.equal(f.sent.at(-1).kind, "retired"); assert.equal(f.sent.at(-1).seq, 1);
});
test("pending permit retirement never claims issued, then matches not_requested", async () => {
    const f = fixture(); await mounted(f); await pressed(f); f.host.isConnected = false; f.mutate(); await tick();
    assert.equal(f.sent.length, 2); await permit(f); settled(f, "not_requested"); assert.equal(f.writes.length, 0);
    await ack(f); await f.running; assert.equal(f.sent.at(-1).kind, "retired");
});
test("lost/mismatched/malformed replies retire without false ownership settlement or replay", async () => {
    for (const reply of [null, "permit", { kind: "permit" }, { kind: "permit", id: 999, mount: options.mount, nonce: options.nonce, seq: 1, url: URL }]) {
        const f = fixture(); await mounted(f); await pressed(f); f.reply(reply); await tick(); await f.running;
        assert.deepEqual(f.sent.map(m => m.kind), ["mounted", "pressed"]); f.click(); await tick();
        assert.equal(f.sent.length, 2); assert.equal(f.writes.length, 0);
    }
    const f = fixture(); await mounted(f); f.failReceive(); await pressed(f); await f.running;
    assert.equal(f.writes.length, 0); assert.equal(f.sent.filter(m => m.kind === "settled").length, 0);
});
test("matching malformed permit URL refuses and retires after ack", async () => {
    const f = fixture(); await mounted(f); await pressed(f); f.reply(f.response("permit", 1, { url: { text: URL } }));
    await tick(); settled(f, "not_requested"); assert.equal(f.writes.length, 0);
    await ack(f); await f.running; f.click(); await tick(); assert.equal(f.writes.length, 0);
});
test("bad ack cannot authorize a fresh event after actual fulfillment", async () => {
    const f = fixture(); await mounted(f); await pressed(f); await permit(f); settled(f, "fulfilled");
    f.reply(f.response("ack", 2)); await tick(); await f.running; f.click(); await tick();
    assert.equal(f.writes.length, 1); assert.equal(f.sent.filter(m => m.kind === "pressed").length, 1);
});
test("missing initial clipboard is unsupported with no handler or copy", async () => {
    const f = fixture({ unsupported: true }); await f.running;
    assert.deepEqual(f.sent.map(m => m.kind), ["unsupported"]); assert.equal(f.sent[0].seq, 0);
    f.click(); assert.equal(f.writes.length, 0); assert.equal(f.observers.length, 0);
});

const watchdog = setTimeout(() => { console.error("Model corpus did not complete"); process.exitCode = 1; }, 5000);
(async () => {
    let passed = 0;
    for (const [name, body] of tests) { await body(); passed += 1; console.log(`ok ${passed} - ${name}`); }
    console.log(`MODEL_ONLY ${passed}/${tests.length}; no native clipboard, browser gesture, OS or NativeAck acceptance`);
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(() => clearTimeout(watchdog));
