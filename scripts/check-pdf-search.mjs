import {test} from 'node:test';
import assert from 'node:assert/strict';
import {pageText, findMatches} from '../src/desktop/pdf_preview/search.mjs';

test('search crosses PDF text items and line breaks without inventing word gaps', () => {
  const page = pageText([{str:'Super ',hasEOL:false},{str:'Platinum',hasEOL:true},{type:'beginMarkedContent'},{str:'Selectable text',hasEOL:false}]);
  assert.equal(page.text, 'Super Platinum\nSelectable text');
  assert.deepEqual(findMatches(page.text, 'PLATINUM selectable'), [[6,25]]);
  assert.deepEqual(page.segments.map(s=>page.text.slice(s.start,s.end)), ['Super ','Platinum','Selectable text']);
  assert.deepEqual(findMatches(pageText([{str:'land'},{str:'scape'}]).text,'landscape'), [[0,9]]);
});

test('queries are literal, Unicode-aware and ignore case', () => {
  const text = 'a+b [ready] file.js 日本語 🦊 ÉCOLE';
  for (const query of ['a+b','[ready]','file.js','日本語','🦊','école']) {
    const matches = findMatches(text,query);
    assert.equal(matches.length,1,query);
    assert.equal(text.slice(...matches[0]).toLowerCase(),query.toLowerCase());
  }
  assert.deepEqual(findMatches(text,'a*b'), []);
  assert.deepEqual(findMatches(text,'.*'), []);
  assert.deepEqual(findMatches(text,'  \n\t'), []);
});

test('nonoverlapping occurrences retain exact source offsets', () => {
  assert.deepEqual(findMatches('ab ab\nAB','ab'), [[0,2],[3,5],[6,8]]);
  assert.deepEqual(findMatches('x\n  y\tz','x y z'), [[0,7]]);
  assert.deepEqual(findMatches('banana','ana'), [[1,4]]);
});
