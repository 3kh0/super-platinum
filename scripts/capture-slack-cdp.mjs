#!/usr/bin/env node
// Zero-dep CDP driver for the real Slack desktop app.
//
// Launch Slack with a debug port first:
//   osascript -e 'quit app "Slack"'; open -a Slack --args --remote-debugging-port=9222
//
// Usage:
//   node scripts/capture-slack-cdp.mjs --list
//   node scripts/capture-slack-cdp.mjs --eval 'document.title'
//   node scripts/capture-slack-cdp.mjs --screenshot out.png
//   node scripts/capture-slack-cdp.mjs [--filter substr] [--reload]   # network capture until Ctrl+C
//
// Output for network capture: captures/slack-<ts>/requests.ndjson + summary.json
import fs from 'node:fs';
import path from 'node:path';

const args = process.argv.slice(2);
const flag = (name, fallback = null) => {
  const i = args.indexOf(name);
  return i === -1 ? fallback : args[i + 1] ?? true;
};
const has = (name) => args.includes(name);
const port = Number(flag('--port', 9222));

async function targets() {
  const res = await fetch(`http://127.0.0.1:${port}/json/list`);
  return res.json();
}

async function rendererTarget() {
  const list = await targets();
  const page = list.find((t) => t.type === 'page' && /app\.slack\.com\/client/.test(t.url || ''))
    ?? list.find((t) => t.type === 'page' && /slack/.test(t.url || ''))
    ?? list.find((t) => t.type === 'page');
  if (!page) {
    console.error('no Slack renderer target; targets:', list.map((t) => `${t.type} ${t.url}`).join('\n'));
    process.exit(1);
  }
  return page;
}

function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  let nextId = 1;
  const pending = new Map();
  const listeners = [];
  const ready = new Promise((resolve, reject) => {
    ws.addEventListener('open', () => resolve());
    ws.addEventListener('error', (e) => reject(e));
  });
  ws.addEventListener('message', (event) => {
    const msg = JSON.parse(event.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
    } else if (msg.method) {
      for (const fn of listeners) fn(msg);
    }
  });
  const send = (method, params = {}, sessionId) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      pending.set(id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params, sessionId }));
    });
  return { ready, send, on: (fn) => listeners.push(fn), close: () => ws.close() };
}

const page = await rendererTarget();
const cdp = connect(page.webSocketDebuggerUrl);
await cdp.ready;

if (has('--list')) {
  console.log((await targets()).map((t) => `${t.type}\t${t.url}`).join('\n'));
  process.exit(0);
}

if (has('--eval')) {
  const expression = String(flag('--eval'));
  const result = await cdp.send('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
    allowUnsafeEvalBlockedByCSP: true,
  });
  const value = result.result?.value;
  console.log(typeof value === 'string' ? value : JSON.stringify(value, null, 1));
  process.exit(0);
}

if (has('--screenshot')) {
  const out = String(flag('--screenshot', 'slack.png'));
  const shot = await cdp.send('Page.captureScreenshot', { format: 'png' });
  fs.mkdirSync(path.dirname(path.resolve(out)), { recursive: true });
  fs.writeFileSync(out, Buffer.from(shot.data, 'base64'));
  console.log(`wrote ${out}`);
  process.exit(0);
}

// Network capture mode.
const stamp = new Date().toISOString().replace(/[:.]/g, '-');
const dir = path.join('captures', `slack-${stamp}`);
fs.mkdirSync(dir, { recursive: true });
const stream = fs.createWriteStream(path.join(dir, 'requests.ndjson'));
const filter = flag('--filter');
const seen = new Map();

await cdp.send('Network.enable');
await cdp.send('Page.enable');
cdp.on(async (msg) => {
  const { method, params } = msg;
  if (method === 'Network.requestWillBeSent') {
    const url = params.request.url;
    if (filter && !url.includes(filter)) return;
    seen.set(params.requestId, url);
    stream.write(JSON.stringify({ phase: 'request', requestId: params.requestId, url, method: params.request.method, postData: params.request.postData }) + '\n');
  } else if (method === 'Network.responseReceived') {
    if (!seen.has(params.requestId)) return;
    stream.write(JSON.stringify({ phase: 'response', requestId: params.requestId, url: params.response.url, status: params.response.status, mimeType: params.response.mimeType }) + '\n');
  } else if (method === 'Network.loadingFinished') {
    if (!seen.has(params.requestId)) return;
    try {
      const body = await cdp.send('Network.getResponseBody', { requestId: params.requestId });
      stream.write(JSON.stringify({ phase: 'responseBody', requestId: params.requestId, url: seen.get(params.requestId), body: body.base64Encoded ? '<base64>' : body.body }) + '\n');
    } catch { /* body already evicted */ }
  }
});

if (has('--reload')) await cdp.send('Page.reload');
console.error(`capturing → ${dir} (Ctrl+C to stop)`);
process.on('SIGINT', () => {
  fs.writeFileSync(path.join(dir, 'summary.json'), JSON.stringify({ count: seen.size, urls: [...new Set(seen.values())] }, null, 1));
  stream.end();
  console.error(`\ncaptured ${seen.size} requests → ${dir}`);
  process.exit(0);
});
