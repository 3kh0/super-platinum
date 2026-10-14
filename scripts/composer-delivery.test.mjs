import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

function fixture() {
  const field = {value:'Sent text', dataset:{draftScope:'A:T:C'}, style:{}, scrollHeight:32};
  const context = {document:{getElementById:id=>id==='composer'?field:null}};
  vm.runInNewContext(readFileSync(new URL('../src/desktop/composer_delivery.js', import.meta.url),'utf8'),context);
  return {field, clear:()=>context.clearDeliveredComposer('composer','A:T:C','Sent text'), replace:()=>context.replaceReviewedComposer('composer','A:T:C','Sent text','Selected local version'), settle:()=>JSON.parse(JSON.stringify(context.settleScheduledComposer('composer','A:T:C','Sent text')))};
}
test('durable delivery clears the matching native composer',()=>{
  const {field,clear}=fixture();
  assert.equal(clear(),true);
  assert.equal(field.value,'');
  assert.equal(field.style.height,'32px');
});

test('scheduling returns the newer native wire without clearing or requiring an input event',()=>{
  const {field,settle}=fixture();
  field.value='New draft 日本語 🚀';
  assert.deepEqual(settle(),{status:'changed',scope:'A:T:C',text:'New draft 日本語 🚀'});
  assert.equal(field.value,'New draft 日本語 🚀');
});
test('scheduling clears only a matching field and returns the exact scope',()=>{
  const {field,settle}=fixture();
  assert.deepEqual(settle(),{status:'cleared',scope:'A:T:C'});
  assert.equal(field.value,'');
});
test('scheduling does not serialize provisional IME text or expose another scope',()=>{
  const {field,settle}=fixture();
  field.value='Provisional 日本語';
  field.dataset.composing='true';
  assert.deepEqual(settle(),{status:'composing'});
  field.dataset.composing='false';field.dataset.draftScope='replacement';
  assert.deepEqual(settle(),{status:'unavailable'});
  assert.equal(field.value,'Provisional 日本語');
});
test('oversized native scheduling receipt preserves the field without sending its content',()=>{
  const {field,settle}=fixture();
  field.value='x'.repeat(131073);
  assert.deepEqual(settle(),{status:'unavailable'});
  assert.equal(field.value.length,131073);
});
test('native typing ahead of the Rust signal survives the delayed clear',()=>{
  const {field,clear}=fixture();
  field.value='Sent text plus newer native input';
  assert.equal(clear(),false);
  assert.equal(field.value,'Sent text plus newer native input');
});
test('same text in a replacement account or channel is not the old composer',()=>{
  const {field,clear}=fixture();
  for(const scope of ['B:T:C','A:T:Other']) {
    field.dataset.draftScope=scope;
    assert.equal(clear(),false);
    assert.equal(field.value,'Sent text');
  }
});
test('composition ownership prevents clearing before its first native input',()=>{
  const {field,clear}=fixture();
  field.dataset.composing='true';
  assert.equal(clear(),false);
  assert.equal(field.value,'Sent text');
});
test('old visit cannot clear, review-replace or settle the reopened same destination',()=>{
  const {field,clear,replace,settle}=fixture();
  field.dataset.draftScope='A:T:C:next-visit';
  assert.equal(clear(),false);
  assert.equal(replace(),false);
  assert.deepEqual(settle(),{status:'unavailable'});
  assert.equal(field.value,'Sent text');
});
test('review choice replaces matching wire text and resizes the field',()=>{
  const {field,replace}=fixture();
  assert.equal(replace(),true);
  assert.equal(field.value,'Selected local version');
  assert.equal(field.style.height,'32px');
});
test('review choice preserves newer native typing, a replacement destination and IME',()=>{
  for(const change of [
    field=>field.value='Native text ahead of Rust',
    field=>field.dataset.draftScope='Other account visit',
    field=>field.dataset.composing='true',
  ]) {
    const {field,replace}=fixture();
    change(field);
    const before=field.value;
    assert.equal(replace(),false);
    assert.equal(field.value,before);
  }
});
