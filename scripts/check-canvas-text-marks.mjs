#!/usr/bin/env node
// Offline, synthetic-only oracle for captured GoiO/YLF4 visitor semantics.
// No Slack entrypoints, network, account state, or real document content.
// The tiny DOM adapter covers only the explicit element/text fixtures below;
// it does not claim browser HTML-repair/parser equivalence or Rust interop.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const bundle = process.argv[2] ?? 'tmp/sb/js/gantry-v2-async-quip-collab-webapp.7f6a8bd5f9d5a08b.min.js';
const output = process.argv[3];
const modules = {};
const ACCEPT = 1, REJECT = 2, SKIP = 3;
function element(tag, children = [], attrs = {}) {
  const node = {
    nodeType: 1, nodeName: tag.toUpperCase(), tagName: tag.toUpperCase(),
    childNodes: children, parentNode: null, dataset: {},
    attributes: Object.entries(attrs).map(([name, value]) => ({ name, value })),
    hasAttributes: () => Object.keys(attrs).length > 0,
    getAttribute: name => attrs[name] ?? null,
    get textContent() { return children.map(child => child.textContent).join(''); },
  };
  for (const child of children) child.parentNode = node;
  return node;
}
function text(data) {
  return { nodeType: 3, nodeName: '#text', data, textContent: data, length: data.length, parentNode: null, childNodes: [] };
}
function walker(root, _what, filter) {
  const nodes = [];
  function visit(parent) {
    for (const child of parent.childNodes) {
      const verdict = filter(child);
      if (verdict === ACCEPT) nodes.push(child);
      if (verdict !== REJECT) visit(child);
    }
  }
  visit(root);
  let index = -1;
  return {
    nextNode: () => nodes[++index] ?? null,
    // The declared fixtures contain no controls, whose traversal skips subtrees.
    nextSibling: () => { throw Error('unexpected control traversal'); },
    parentNode: () => { throw Error('unexpected control traversal'); },
  };
}
const dom = {
  cw: { ELEMENT: 1, TEXT: 4 }, Z6: { ELEMENT: 1, TEXT: 3 }, UU: { ACCEPT, REJECT, SKIP },
  ir: node => node.nodeType === 3, vq: node => node.nodeType === 1, IR: node => node.nodeType === 1,
  t8: () => { throw Error('This oracle accepts explicit synthetic DOM only'); },
};
const context = vm.createContext({
  Node: { ELEMENT_NODE: 1 }, document: { createTreeWalker: walker },
  rspackChunkGantryV2: { push(chunk) { Object.assign(modules, chunk[1]); } },
});
vm.runInContext(fs.readFileSync(bundle, 'utf8'), context, { timeout: 5000 });
const stubs = {
  nW3w: { vA: condition => assert.ok(condition, 'captured visitor assertion') },
  pg2g: dom, 'qa+r': { cw: value => !value.trim() }, aNWE: { kT: ['http', 'https'] },
  mQqb: { S: { error: () => { throw Error('captured visitor failed'); }, warn: () => { throw Error('unexpected RTML shape'); } } },
};
const cache = new Map();
function require(id) {
  if (Object.hasOwn(stubs, id)) return stubs[id];
  assert.ok(['GoiO', 'YLF4'].includes(id), `unexpected module ${id}`);
  if (cache.has(id)) return cache.get(id).exports;
  assert.equal(typeof modules[id], 'function', `captured module ${id}`);
  const module = { exports: {} }; cache.set(id, module);
  modules[id](module, module.exports, require);
  return module.exports;
}
require.d = (target, getters = {}, values = {}) => {
  for (const [name, get] of Object.entries(getters)) Object.defineProperty(target, name, { enumerable: true, get });
  for (const [name, value] of Object.entries(values)) Object.defineProperty(target, name, { enumerable: true, get: () => value });
};
const { N: visit } = require('GoiO');
const mark = (name, start, end, value = true) => ({ name, value, start, end });
const cases = [
  { name: 'bold_unicode_entity_softbreak', rtml: '<b>A&amp;😀<br>B</b>',
    nodes: [element('b', [text('A&😀'), element('br'), text('B')])],
    expected: { text: 'A&amp;😀<br>B', marks: [mark('b', 0, 13)] } },
  { name: 'utf16_prefix', rtml: '前😀<b>a&amp;</b>',
    nodes: [text('前😀'), element('b', [text('a&')])],
    expected: { text: '前😀a&amp;', marks: [mark('b', 3, 9)] } },
  { name: 'link_unicode_entity_softbreak', rtml: '<a href="https://example.test/?a=1&amp;b=2">go😀<br>&amp;</a>',
    nodes: [element('a', [text('go😀'), element('br'), text('&')], { href: 'https://example.test/?a=1&b=2' })],
    expected: { text: 'go😀<br>&amp;', marks: [mark('a', 0, 13, JSON.stringify({ href: 'https://example.test/?a=1&b=2' }))] } },
  { name: 'nested_marks', rtml: '<b><i>😀</i>&amp;</b>',
    nodes: [element('b', [element('i', [text('😀')]), text('&')])],
    expected: { text: '😀&amp;', marks: [mark('i', 0, 2), mark('b', 0, 7)] } },
  { name: 'control_normalization', rtml: 'a\tb\nc\r\u0001\u0085',
    nodes: [text('a\tb\nc\r\u0001\u0085')], expected: { text: 'a b c ', marks: [] } },
  { name: 'bold_break_only', rtml: '<b><br></b>', nodes: [element('b', [element('br')])],
    expected: { text: '<br>', marks: [] } },
  { name: 'link_break_only', rtml: '<a href="https://example.test"><br></a>',
    nodes: [element('a', [element('br')], { href: 'https://example.test' })], expected: { text: '<br>', marks: [] } },
];
const result = cases.map(({ name, rtml, nodes, expected }) => {
  const actual = JSON.parse(JSON.stringify(visit(element('root', nodes))));
  assert.deepEqual(actual, expected, name);
  return { name, rtml, ...actual };
});
assert.deepEqual([...cache.keys()].sort(), ['GoiO', 'YLF4']);
if (output) fs.writeFileSync(output, JSON.stringify(result, null, 2) + '\n');
console.log(JSON.stringify({ ok: true, syntheticCases: result.length, capturedModules: [...cache.keys()].sort(),
  utf16EscapedOffsets: true, formattedBreakOnlyHasNoMark: true, htmlParserEquivalence: false, rustInterop: false }));
