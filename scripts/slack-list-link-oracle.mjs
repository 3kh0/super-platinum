#!/usr/bin/env node
// Synthetic, offline source oracle. Registers captured module definitions only;
// never executes the Slack application entrypoint or supplies sessions/network.
// Usage: node scripts/slack-list-link-oracle.mjs [output.json] [--export-patterns]
// The optional flag regenerates fixed regex data and the core synthetic oracle.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

// Node otherwise includes an entire minified source line in VM exceptions.
process.on('uncaughtException', error => {
  console.error(`Link oracle failed: ${error.message}`);
  console.error(String(error.stack).split('\n').filter(line => line.trim().startsWith('at ') && line.length < 400).join('\n'));
  process.exitCode = 1;
});

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const output = path.resolve(process.argv[2] ?? path.join(root, 'tmp/slack-list-link-oracle.json'));
const bundles = [
  'gantry-v2-vendors.522a064003a98b05.min.js',
  'gantry-v2-shared.0b81595876785d7a.min.js',
  'gantry-v2-async-gantry-v2-shared-boot-async.56e901654548b2fe.min.js',
  'gantry-v2-async-gantry-v2-vendors-async.daa946d0953564d3.min.js',
];
const modules = Object.create(null);
const context = vm.createContext({ URL, TextEncoder, TextDecoder });
// PSzk uses the standard URL constructor and a base for relative input. This
// synthetic base is never navigated and exposes no browser/session facilities.
context.window = { URL, location: { href: 'https://example.invalid/oracle/' } };
context.rspackChunkGantryV2 = { push(chunk) { Object.assign(modules, chunk[1]); } };
const sources = bundles.map(name => {
  const source = fs.readFileSync(path.join(root, 'tmp/sb/js', name), 'utf8');
  vm.runInContext(source, context, { timeout: 5000, filename: name });
  return { name, sha256: createHash('sha256').update(source).digest('hex') };
});
const cache = new Map();
function requireModule(id) {
  // uDYD imports the application logger but its valid Link branch never calls
  // it. Refuse a logger invocation rather than boot app state/beacon helpers.
  if (id === 'LXnb') return { A() { throw new Error('Unexpected application logger invocation'); } };
  if (cache.has(id)) return cache.get(id).exports;
  if (!modules[id]) throw new Error(`Missing captured module ${id}`);
  const module = { exports: {} };
  cache.set(id, module);
  modules[id](module, module.exports, requireModule);
  return module.exports;
}
requireModule.d = (target, getters = {}, values = {}) => {
  for (const [key, get] of Object.entries(getters)) Object.defineProperty(target, key, { enumerable: true, get });
  for (const [key, value] of Object.entries(values)) Object.defineProperty(target, key, { enumerable: true, get: () => value });
};
requireModule.r = target => Object.defineProperty(target, '__esModule', { value: true });
requireModule.o = (target, key) => Object.hasOwn(target, key);
requireModule.g = context;
requireModule.j = 'gantry-v2-manifest';
context.requireCaptured = requireModule;

// Fixed synthetic inputs only. No arbitrary file/body/session ingestion.
const inputs = [
  ['', undefined], ['   ', undefined], ['not a link', undefined],
  ['  not a link  ', ''], ['\t\n', undefined],
  ['https://example.com/spec', 'Specification'],
  ['  https://example.com/spec  ', '  Kept name  '],
  ['example.com', undefined], ['www.example.com/spec', undefined],
  ['EXAMPLE.COM', undefined], ['example.invalid', undefined],
  ['http://example.invalid/a', undefined], ['localhost', undefined],
  ['http://localhost:8080/a', undefined], ['192.0.2.1/a', undefined],
  ['https://example.com:443/a?x=1&y=2#part', undefined],
  ['https://example.com/a%20b', undefined],
  ['Read https://example.com/first then https://example.org/second', undefined],
  ['https://example.org/second https://example.com/first', undefined],
  ['(https://example.com/a)', undefined],
  ['https://example.com/a(b)', undefined],
  ['https://example.com/a(b)).', undefined],
  ['https://example.com/end.,;!?', undefined],
  ["'https://example.com/a'", undefined],
  ['a@example.com', undefined], ['mailto:a@example.com', undefined],
  ['a@example.com then https://example.org', undefined],
  ['https://example.org then a@example.com', undefined],
  ['+1 (202) 555-0100', undefined], ['202-555-0100', undefined],
  ['tel:+12025550100', undefined], ['2025550100', undefined],
  ['202-555-0100 then example.com', undefined],
  ['example.com then 202-555-0100', undefined],
  ['go/example', undefined], ['ftp://example.com/file', undefined],
  ['slack://channel?team=T00000001&id=C00000001', undefined],
  ['javascript:alert(1)', undefined], ['data:text/plain,example', undefined],
  ['file:///tmp/example', undefined], ['custom://example/path', undefined],
  ['https://例え.テスト/道', '日本語'], ['文https://example.com/道文', undefined],
  ['😀 https://example.com/😀', '😀'],
  ['\u00a0https://example.com\u00a0', undefined],
  ['\u200bhttps://example.com\u200b', undefined],
  ['https://example.com\nhttps://example.org', undefined],
  ['example.com', '<untrusted display name>'],
];
// Seeded differential corpus: challenge the whole detector, not just positive
// URLs. Cases stay short here; the Rust ignored timing test owns long inputs.
let seed = 0x51ac2026;
const pick = values => {
  seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
  return values[seed % values.length];
};
const stems = ['example.com', 'example.cc', 'example.Cc', 'example.CC', 'example.co.uk',
  'example.museum', 'example.engineering', 'example.invalid', 'example.now', 'ms.now',
  'www.example.invalid', '_example.com', 'under_score.example.com', 'example..com',
  'a@example.com', '_a@example.com', 'git@example.com:repo', 'a+tag@example.travel',
  'x@@example.com', 'https://example.com', 'http://localhost:8080', '//example.com',
  'ftp://example.com', 'sftp://example.com', 'prefixhttp://example.com',
  'javascript:example.com', 'vbscript:example.com', 'mailto:a@example.com',
  'rdar://123&456', 'daylite://Command=ShowObject&id=12', 'hansoft://example.test/a',
  'spotify:track:abc123', 'slack://channel?id=C00000001', 'go/example',
  'https://例え.テスト', 'https://éxample.com', 'https://example.com/😀',
  '202-555-0100', '+1 (202) 555-0100', '(202) 555.0100', '2025550100',
  '+44 20 7946 0958', '090-1234-5678', 'tel:+12025550100', 'no_match'];
const prefixes = ['', ' ', '\t', '\n', '(', '[', '{', '<', '|', "'", '"',
  '@', '/', '_', '*', 'Before ', '道', '😀', '\u200b', '\u00a0', '\u0085'];
const suffixes = ['', '/', '/a', '/a(b)', '/a(b)).', '/道文', '/😀', '/x_y',
  ':8080/a', '.not_a_tld', '..', '?!', '.', ',', "'", ')', ']', '}', '|label>',
  '?x=1&y=2#p', ' tail', '\n', '\u200b', '\u00a0', '道', '@other.com'];
for (let i = 0; i < 1500; i++) {
  let input = pick(prefixes) + pick(stems) + pick(suffixes);
  if (i % 3 === 0) input += pick([' ', ' / ', '\n', '道']) + pick(stems) + pick(suffixes);
  inputs.push([input, i % 7 === 0 ? pick(['', 'Label', '  ', '道😀']) : undefined]);
}
context.syntheticInputs = inputs;
const cases = vm.runInContext(`(() => {
  const normalize = requireCaptured('VTjO').d;
  const detect = requireCaptured('v+tG').A;
  const serialize = requireCaptured('uDYD').a;
  return syntheticInputs.map(([input, name]) => ({
    input, ...(name !== undefined ? {name} : {}),
    matches: detect({input: input.trim()}),
    normalized: normalize(input, name),
    serializedCamelCase: serialize({type:'link',value:normalize(input,name)}),
  }));
})()`, context, { timeout: 10000 });
// Assert anchors rather than merely emit whatever a wrong module might return.
const clean = JSON.parse(JSON.stringify(cases));
const byInput = input => clean.find(item => item.input === input).normalized;
assert.equal(byInput(''), '');
assert.equal(byInput('example.com').originalUrl, 'http://example.com');
assert.equal(byInput('  not a link  ').originalUrl, '  not a link  ');
assert.equal(byInput('a@example.com').originalUrl, 'mailto:a@example.com');
assert.equal(byInput('2025550100').originalUrl, '2025550100');
assert.equal(byInput('go/example').originalUrl, 'go/example');
assert.equal(byInput('https://example.com/spec').displayName, 'Specification');
assert.deepEqual(clean.find(item => item.input === '').serializedCamelCase, {link: []});
assert.deepEqual(clean.find(item => item.input === 'example.com').serializedCamelCase,
  {link: [{originalUrl: 'http://example.com', displayAsUrl: false}]});

context.syntheticStored = [
  null, {}, [], [{original_url:'https://example.com', display_as_url:true, display_name:'Example'}],
  {original_url:'https://example.com', display_as_url:true, display_name:''},
  {originalUrl:'https://example.com', displayAsUrl:null, displayName:null},
  {originalUrl:'https://example.com', displayAsUrl:false, displayName:'  '},
  {originalUrl:'', displayAsUrl:true, displayName:'Ignored'},
  {original_url:'https://example.com', attachment:{synthetic:true}, unknown:'retained only in snapshot'},
  [{originalUrl:'https://example.com/first'}, {originalUrl:'https://example.org/ignored'}],
];
const storedCases = JSON.parse(JSON.stringify(vm.runInContext(`(() => {
  const serialize = requireCaptured('uDYD').a;
  return syntheticStored.map(value => ({value, serializedCamelCase:serialize({type:'link',value})}));
})()`, context, {timeout: 10000})));
assert.deepEqual(storedCases[3].serializedCamelCase,
  {link:[{originalUrl:'https://example.com',displayAsUrl:true,displayName:'Example'}]});
assert.deepEqual(storedCases[8].serializedCamelCase,
  {link:[{originalUrl:'https://example.com',displayAsUrl:false}]});

// Optional reproducible data export for the Rust port. Inject only an export
// statement after captured constants initialize; matching code stays untouched.
if (process.argv[3] === '--export-patterns') {
  const injections = [
    ['vp1n', 'function detectUrls(', 't.__patterns={url:w,suffix:T,protocol:v,basic:C,underscore:O,extension:R,spotify:N,uncommon:f};function detectUrls('],
    ['dVMb', 'function isGitSshUrl(', 't.__patterns={email:s,tlds:i,delimiters:o.ay};function isGitSshUrl('],
    ['xt89', 'function isSurroundedByWhitespaceOrSpecificPunctuation(', 't.__patterns={phone:s};function isSurroundedByWhitespaceOrSpecificPunctuation('],
    ['CwFm', 'n.d(t,{},{A:', 't.__patterns={cjk:o};n.d(t,{},{A:'],
  ];
  const extracted = {};
  for (const [id, anchor, replacement] of injections) {
    let source = modules[id].toString();
    if (!source.includes(anchor)) throw new Error(`Missing export anchor ${id}`);
    source = source.replace(/^[^(]+\(/, 'function(').replace(anchor, replacement);
    const factory = vm.runInContext(`(${source})`, context, {timeout:5000});
    const module = {exports:{}};
    factory(module,module.exports,requireModule);
    for (const [key,value] of Object.entries(module.exports.__patterns)) {
      extracted[key] = value && typeof value.source === 'string'
        ? {source:value.source,flags:value.flags.replace('g','')} : value;
    }
  }
  const tlds = requireModule('NS8g');
  extracted.common_tlds=tlds.Ay;
  extracted.country_tlds=tlds.tU;
  const destination=path.join(root,'crates/super-platinum-core/src/slack_lists/cell_edit/link/detector/patterns.json');
  fs.mkdirSync(path.dirname(destination),{recursive:true});
  fs.writeFileSync(destination,JSON.stringify(extracted,null,2)+'\n');
  fs.writeFileSync(path.join(path.dirname(destination),'oracle.json'),JSON.stringify({
    sources,cases:clean.map(({input,normalized})=>({input,normalized})),
  },null,2)+'\n');
}

fs.mkdirSync(path.dirname(output), { recursive: true });
fs.writeFileSync(output, JSON.stringify({
  syntheticOnly: true,
  sourceModules: {normalizer: 'VTjO', detector: 'v+tG', serializer: 'uDYD'},
  sources,
  cases: clean,
  storedCases,
  loadedModules: [...cache.keys()].sort(),
  limitations: [
    'Captured detector, normalizer and pre-snake-case cell converter; no native implementation or live Slack writes.',
    'Application logger dependency is blocked; its invocation fails the oracle.',
    'Node WHATWG URL with synthetic example.invalid base supplies the captured PSzk browser URL helper.',
    'No assertion that a stored URL is safe to open; native scheme policy stays separate.',
    'No server response/enrichment or List mutation lifecycle coverage.',
  ],
}, null, 2) + '\n', { mode: 0o600 });
console.log(JSON.stringify({ output, cases: clean.length, storedCases:storedCases.length, modules: cache.size, syntheticOnly: true }));
